package com.drawdock

import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.view.Surface
import org.json.JSONArray
import org.json.JSONObject
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.IOException
import java.net.InetSocketAddress
import java.security.MessageDigest
import java.security.SecureRandom
import java.security.cert.CertificateException
import java.security.cert.X509Certificate
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLSocket
import javax.net.ssl.TrustManager
import javax.net.ssl.X509TrustManager
import kotlin.concurrent.thread

class SessionClient(
    private val host: String,
    private val port: Int,
    private val fingerprint: String,
    private val invitationToken: String,
    private val onStatus: (String, Boolean) -> Unit,
    private val onModeAck: (String, Long) -> Unit,
    private val onClearAck: (Long) -> Unit,
    private val onEditorEvent: (JSONObject) -> Unit,
    private val videoSurface: Surface,
    private val onSources: (JSONArray, JSONObject) -> Unit,
    private val onSourceSelected: (JSONObject, Long) -> Unit,
    private val onSourceReady: (Long) -> Unit,
    private val onVideoGeometry: (Long, Int, Int) -> Unit,
    private val onVideoRendered: (Long) -> Unit,
    private val onVideoError: (String) -> Unit,
    private val rememberedDeviceId: String?,
    private val onRemembered: (String, String) -> Unit,
    private val onZoomSelected: (JSONObject, Long) -> Unit,
    private val onZoomCapability: (Boolean) -> Unit,
) {
    var viewport = SharedViewport()
    private val mainHandler = Handler(Looper.getMainLooper())
    private val closed = AtomicBoolean(false)
    private val credentialLock = Any()
    private val commands = SessionOutbox()
    private val queuedBytes = AtomicLong(0)
    private val nextObjectId = AtomicLong(SystemClock.elapsedRealtimeNanos())
    private val nextModeRequestId = AtomicLong(0)
    private val nextClearRequestId = AtomicLong(0)
    private val nextSourceRequestId = AtomicLong(0)
    private val nextEditorRequestId = AtomicLong(1_000_000_000)
    @Volatile private var sourceEpoch = 0L
    @Volatile private var videoSocket: SSLSocket? = null
    private val pointerLock = Any()
    private var pointerGeneration = 0L
    private var activePointerGeneration: Long? = null
    @Volatile private var socket: SSLSocket? = null
    @Volatile private var output: BufferedOutputStream? = null

    fun connect() {
        postStatus("Connecting with pinned TLS…", false)
        thread(name = "drawdock-tls-connect", isDaemon = true) {
            try {
                val expectedFingerprint = parseFingerprint(fingerprint)
                val trustManager = PinnedTrustManager(expectedFingerprint)
                val context = SSLContext.getInstance("TLS")
                context.init(null, arrayOf<TrustManager>(trustManager), SecureRandom())
                val connection = context.socketFactory.createSocket() as SSLSocket
                socket = connection
                if (closed.get()) {
                    connection.close()
                    return@thread
                }
                connection.connect(InetSocketAddress(host, port), 5_000)
                connection.tcpNoDelay = true
                connection.soTimeout = 10_000
                connection.startHandshake()

                val streamIn = BufferedInputStream(connection.inputStream)
                val streamOut = BufferedOutputStream(connection.outputStream)
                writeFrame(
                    streamOut,
                    JSONObject()
                        .put("type", if (rememberedDeviceId == null) "pair" else "resume")
                        .put("device_id", rememberedDeviceId)
                        .put("token", invitationToken)
                        .put("client", Build.MODEL.take(64)),
                )
                connection.soTimeout = 300_000
                val response = readFrame(streamIn)
                if (response.optString("type") != "paired") {
                    throw SecurityException(response.optString("message", "Host did not approve this tablet session"))
                }
                require(response.optInt("version") == 2) {
                    "Host protocol is incompatible; rebuild and restart the host"
                }
                synchronized(credentialLock) {
                    if (closed.get()) return@thread
                    val credential = response.optJSONObject("remembered_device")
                    check(rememberedDeviceId != null || credential != null) {
                        "PC host does not support remembered devices; rebuild and restart it"
                    }
                    credential?.let {
                        onRemembered(credential.getString("id"), credential.getString("token"))
                    }
                }

                connection.soTimeout = 0
                val sources = response.getJSONArray("sources")
                val selected = response.getJSONObject("selected")
                mainHandler.post { onZoomCapability(response.optBoolean("shared_zoom")) }
                sourceEpoch = selected.getLong("source_epoch")
                mainHandler.post { onSources(sources, selected) }
                output = streamOut
                postStatus("Connected (TLS certificate pinned)", true)
                thread(name = "drawdock-session-writer", isDaemon = true) { writeLoop() }
                thread(name = "drawdock-session-reader", isDaemon = true) {
                    readLoop(streamIn)
                }
                val videoToken = response.getString("video_token")
                thread(name = "drawdock-video-reader", isDaemon = true) {
                    try {
                        val video = context.socketFactory.createSocket() as SSLSocket
                        videoSocket = video
                        if (closed.get()) { video.close(); return@thread }
                        video.connect(InetSocketAddress(host, port), 5_000)
                        video.tcpNoDelay = true
                        video.soTimeout = 5_000
                        video.startHandshake()
                        val videoOut = BufferedOutputStream(video.outputStream)
                        writeFrame(videoOut, JSONObject().put("type", "video").put("token", videoToken))
                        RemoteVideoDecoder(
                            videoSurface,
                            { epoch, width, height -> mainHandler.post { onVideoGeometry(epoch, width, height) } },
                            onVideoRendered,
                        ).run(BufferedInputStream(video.inputStream))
                    } catch (error: Exception) {
                        if (!closed.get()) mainHandler.post {
                            onVideoError(error.message ?: "Video connection ended")
                        }
                    } finally {
                        try { videoSocket?.close() } catch (_: IOException) {
                            // The decoder/transport error is already surfaced above.
                        }
                    }
                }
            } catch (error: Exception) {
                close()
                postStatus("Connection failed: ${error.message ?: error.javaClass.simpleName}", false)
            }
        }
    }

    fun sendMark(mark: InkMark, viewportWidth: Int, viewportHeight: Int): Boolean {
        if (viewportWidth <= 0 || viewportHeight <= 0 || mark.points.isEmpty()) return false
        val reserved = mark.points.size.toLong() * 8L + 256L +
            (mark.imageData?.length ?: 0) + (mark.text?.toByteArray(Charsets.UTF_8)?.size ?: 0)
        while (true) {
            val current = queuedBytes.get()
            if (current + reserved > MAX_PENDING_BYTES) {
                postStatus("Pending stroke budget is full; stroke was not sent", socket != null)
                return false
            }
            if (queuedBytes.compareAndSet(current, current + reserved)) break
        }
        val command = SessionCommand.Commit(viewport.mark(mark, viewportWidth, viewportHeight),
            viewportWidth, viewportHeight, reserved, sourceEpoch)
        if (!enqueue(command)) {
            queuedBytes.addAndGet(-reserved)
            return false
        }
        return true
    }

    fun undo() = enqueue(SessionCommand.Control("undo", sourceEpoch))
    fun redo() = enqueue(SessionCommand.Control("redo", sourceEpoch))
    fun sourceReady(epoch: Long) = enqueue(SessionCommand.Control("source_ready", epoch))
    fun editor(message: JSONObject): Long? {
        val requestId = nextEditorRequestId.incrementAndGet()
        message.put("request_id", requestId)
        return requestId.takeIf { enqueue(SessionCommand.Editor(message.toString(), sourceEpoch)) }
    }

    fun zoom(view: SharedViewport): Long? = editor(JSONObject().put("type", "set_zoom")
        .put("zoom", JSONObject().put("scale", view.scale.toDouble())
            .put("left", view.left.toDouble()).put("top", view.top.toDouble())))

    fun sendLaser(active: Boolean, x: Float, y: Float, color: Int, width: Int, height: Int): Boolean {
        if (width <= 0 || height <= 0) return false
        val point = viewport.source((x / width).coerceIn(0f, 1f), (y / height).coerceIn(0f, 1f))
        return enqueue(SessionCommand.Laser(active, point.first, point.second, color, sourceEpoch))
    }
    fun selectSource(name: String): Long? {
        val requestId = nextSourceRequestId.incrementAndGet()
        return requestId.takeIf { enqueue(SessionCommand.SelectSource(name, requestId, sourceEpoch)) }
    }
    fun clearDisplay(): Long? {
        val requestId = nextClearRequestId.incrementAndGet()
        return requestId.takeIf {
            enqueue(SessionCommand.ClearDisplay(requestId, sourceEpoch))
        }
    }

    fun requestMode(mode: String): Long? {
        if (mode != "draw" && mode != "pointer") return null
        val requestId = nextModeRequestId.incrementAndGet()
        return requestId.takeIf {
            enqueue(SessionCommand.Mode(mode, requestId, sourceEpoch))
        }
    }

    fun sendPointer(event: PointerEvent, viewportWidth: Int, viewportHeight: Int): Boolean {
        if (viewportWidth <= 0 || viewportHeight <= 0) return false
        return synchronized(pointerLock) {
            val generation = when (event.action) {
                "down" -> {
                    if (activePointerGeneration != null) return false
                    pointerGeneration + 1
                }
                "move" -> activePointerGeneration ?: pointerGeneration
                "up", "cancel" -> {
                    val active = activePointerGeneration ?: return false
                    active
                }
                "hover" -> pointerGeneration
                else -> return false
            }
            val command = SessionCommand.Pointer(
                event.action,
                generation,
                (viewport.left + event.x / viewportWidth / viewport.scale) * viewportWidth,
                (viewport.top + event.y / viewportHeight / viewport.scale) * viewportHeight,
                event.button,
                viewportWidth,
                viewportHeight,
                sourceEpoch,
            )
            if (!enqueue(command)) return false
            when (event.action) {
                "down" -> {
                    pointerGeneration = generation
                    activePointerGeneration = generation
                }
                "up", "cancel" -> activePointerGeneration = null
            }
            true
        }
    }

    fun disconnect() {
        close()
        postStatus("Disconnected", false)
    }

    private fun enqueue(command: SessionCommand): Boolean {
        if (closed.get()) return false
        if (!commands.offer(command)) {
            postStatus("Session queue full; command was not sent", socket != null)
            return false
        }
        return true
    }

    private fun writeLoop() {
        while (!closed.get()) {
            try {
                // Wait before dequeueing so motion can coalesce until it is sent.
                if (!waitForSendSlot()) return
                val command = commands.poll(500)
                val stream = output ?: return
                if (command == null) {
                    if (!writeSessionFrame(stream, JSONObject().put("type", "heartbeat"))) return
                } else {
                    try {
                        for (message in encodeCommand(command)) {
                            message.put("source_epoch", command.sourceEpoch)
                            if (!writeSessionFrame(stream, message)) return
                        }
                    } finally {
                        if (command is SessionCommand.Commit) {
                            queuedBytes.addAndGet(-command.reservedBytes)
                        }
                    }
                }
            } catch (error: Exception) {
                if (!closed.get()) {
                    postStatus("Session write failed: ${error.message ?: "I/O error"}", false)
                }
                close()
                return
            }
        }
    }

    private fun waitForSendSlot(): Boolean {
        while (!closed.get()) {
            val delay = commands.delayBeforeFrame(System.nanoTime())
            if (delay == 0L) return true
            TimeUnit.NANOSECONDS.sleep(delay)
        }
        return false
    }

    private fun writeSessionFrame(stream: BufferedOutputStream, message: JSONObject): Boolean {
        if (!waitForSendSlot()) return false
        writeFrame(stream, message)
        commands.frameWritten(System.nanoTime())
        return true
    }

    private fun readLoop(input: BufferedInputStream) {
        try {
            while (!closed.get()) {
                val response = readFrame(input)
                when (response.optString("type")) {
                    "error" -> {
                        mainHandler.post { onEditorEvent(response) }
                        postStatus("Host rejected command: ${response.optString("message")}", true)
                    }
                    "editor_ack", "ack" -> {
                        if (response.has("canvas") || response.optString("type") == "editor_ack") {
                            mainHandler.post { onEditorEvent(response) }
                        }
                    }
                    "mode_ack" -> {
                        val mode = response.optString("mode")
                        val requestId = response.optLong("request_id", -1)
                        if (mode in listOf("draw", "pointer") && requestId > 0) {
                            mainHandler.post { onModeAck(mode, requestId) }
                        }
                    }
                    "clear_ack" -> {
                        val requestId = response.optLong("request_id", -1)
                        if (requestId > 0) mainHandler.post {
                            onClearAck(requestId)
                            onEditorEvent(response)
                        }
                    }
                    "source_selected" -> {
                        val selected = response.getJSONObject("selected")
                        sourceEpoch = selected.getLong("source_epoch")
                        val requestId = response.getLong("request_id")
                        mainHandler.post { onSourceSelected(selected, requestId) }
                    }
                    "zoom_selected" -> {
                        val selected = response.getJSONObject("selected")
                        sourceEpoch = selected.getLong("source_epoch")
                        val requestId = response.getLong("request_id")
                        mainHandler.post { onZoomSelected(selected, requestId) }
                    }
                    "source_ready_ack" -> {
                        val epoch = response.getLong("source_epoch")
                        mainHandler.post { onSourceReady(epoch); onEditorEvent(response) }
                    }
                }
            }
        } catch (error: Exception) {
            if (!closed.get()) {
                postStatus("Host session ended: ${error.message ?: "connection closed"}", false)
            }
            close()
        }
    }

    private fun close() {
        if (!synchronized(credentialLock) { closed.compareAndSet(false, true) }) return
        val connections = listOfNotNull(socket, videoSocket)
        socket = null
        videoSocket = null
        output = null
        commands.clear()
        queuedBytes.set(0)
        // TLS close can write close_notify; lifecycle callbacks must not do network I/O.
        thread(name = "drawdock-session-close", isDaemon = true) {
            for (connection in connections) {
                try {
                    connection.close()
                } catch (_: IOException) {
                    // Closing a failed socket is best-effort; the original failure is reported.
                }
            }
        }
    }

    private fun postStatus(message: String, connected: Boolean) {
        mainHandler.post { onStatus(message, connected) }
    }

    private fun writeFrame(output: BufferedOutputStream, message: JSONObject) {
        val bytes = message.toString().toByteArray(Charsets.UTF_8)
        if (bytes.size >= MAX_FRAME_BYTES) throw IllegalArgumentException("Message exceeds 64 KiB")
        output.write(bytes)
        output.write('\n'.code)
        output.flush()
    }

    private fun encodeCommand(command: SessionCommand): List<JSONObject> = when (command) {
        is SessionCommand.Control -> listOf(JSONObject().put("type", command.type))
        is SessionCommand.Editor -> listOf(JSONObject(command.json))
        is SessionCommand.Laser -> listOf(JSONObject().put("type", "laser")
            .put("active", command.active).put("x", command.x.toDouble()).put("y", command.y.toDouble())
            .put("rgba", JSONArray().put(android.graphics.Color.red(command.color))
                .put(android.graphics.Color.green(command.color))
                .put(android.graphics.Color.blue(command.color)).put(255)))
        is SessionCommand.SelectSource -> listOf(
            JSONObject().put("type", "select_source").put("source", command.source)
                .put("request_id", command.requestId),
        )
        is SessionCommand.Mode -> listOf(
            JSONObject()
                .put("type", "set_mode")
                .put("mode", command.mode)
                .put("request_id", command.requestId),
        )
        is SessionCommand.ClearDisplay -> listOf(
            JSONObject()
                .put("type", "clear_display")
                .put("request_id", command.requestId),
        )
        is SessionCommand.Pointer -> listOf(
            JSONObject()
                .put("type", "pointer")
                .put("action", command.action)
                .put("generation", command.generation)
                .put("x", (command.x / command.viewportWidth).coerceIn(0f, 1f).toDouble())
                .put("y", (command.y / command.viewportHeight).coerceIn(0f, 1f).toDouble())
                .put("button", command.button),
        )
        is SessionCommand.Commit -> encodeCommit(command)
    }

    private fun encodeCommit(command: SessionCommand.Commit): List<JSONObject> {
        val id = nextObjectId.incrementAndGet()
        val points = JSONArray()
        command.mark.commitPoints().forEach { (x, y) ->
            points.put(
                JSONObject()
                    .put("x", (x / command.viewportWidth).coerceIn(0f, 1f).toDouble())
                    .put("y", (y / command.viewportHeight).coerceIn(0f, 1f).toDouble()),
            )
        }
        val rgba = JSONArray()
            .put(android.graphics.Color.red(command.mark.color))
            .put(android.graphics.Color.green(command.mark.color))
            .put(android.graphics.Color.blue(command.mark.color))
            .put(android.graphics.Color.alpha(command.mark.color))
        val objectValue = JSONObject()
            .put("id", id)
            .put("tool", command.mark.tool.name.lowercase())
            .put(
                "style",
                JSONObject()
                    .put("rgba", rgba)
                    .put(
                        "width",
                        (command.mark.width /
                            minOf(command.viewportWidth, command.viewportHeight)).toDouble(),
                    ),
            )
            .put("points", points)
            .put("text", command.mark.text)
            .put("image_data", command.mark.imageData)
        val objectBytes = objectValue.toString().toByteArray(Charsets.UTF_8)
        if (objectBytes.size < MAX_FRAME_BYTES) {
            return listOf(JSONObject().put("type", "commit").put("object", objectValue))
        }
        if (objectBytes.size > MAX_COMMIT_BYTES) {
            throw IllegalArgumentException("Stroke exceeds the 512 KiB commit limit")
        }
        val digest = MessageDigest.getInstance("SHA-256")
            .digest(objectBytes)
            .joinToString("") { "%02x".format(it.toInt() and 0xff) }
        val messages = ArrayList<JSONObject>()
        messages.add(
            JSONObject()
                .put("type", "begin_commit")
                .put("object_id", id)
                .put("total_bytes", objectBytes.size)
                .put("digest", digest),
        )
        var offset = 0
        while (offset < objectBytes.size) {
            val end = minOf(offset + MAX_COMMIT_CHUNK_BYTES, objectBytes.size)
            val chunk = objectBytes.copyOfRange(offset, end)
            messages.add(
                JSONObject()
                    .put("type", "commit_chunk")
                    .put("object_id", id)
                    .put("offset", offset)
                    .put("data", android.util.Base64.encodeToString(chunk, android.util.Base64.NO_WRAP)),
            )
            offset = end
        }
        messages.add(JSONObject().put("type", "finish_commit").put("object_id", id))
        return messages
    }

    private fun readFrame(input: BufferedInputStream): JSONObject {
        val bytes = ArrayList<Byte>(256)
        while (bytes.size < MAX_FRAME_BYTES) {
            val value = input.read()
            if (value < 0) throw IllegalStateException("Host closed the connection")
            if (value == '\n'.code) {
                return JSONObject(String(ByteArray(bytes.size) { bytes[it] }, Charsets.UTF_8))
            }
            bytes.add(value.toByte())
        }
        throw IllegalArgumentException("Host message exceeds 64 KiB")
    }

    private fun parseFingerprint(value: String): ByteArray {
        val normalized = value.replace(":", "").replace(" ", "").lowercase()
        if (normalized.length != 64 || normalized.any { it !in "0123456789abcdef" }) {
            throw IllegalArgumentException("Enter the full 64-digit SHA-256 fingerprint")
        }
        return ByteArray(32) { index ->
            normalized.substring(index * 2, index * 2 + 2).toInt(16).toByte()
        }
    }

    private class PinnedTrustManager(private val expected: ByteArray) : X509TrustManager {
        override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?) {
            throw CertificateException("Client certificates are not accepted")
        }

        override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?) {
            val certificate = chain?.firstOrNull()
                ?: throw CertificateException("Host did not present a certificate")
            val actual = MessageDigest.getInstance("SHA-256").digest(certificate.encoded)
            if (!MessageDigest.isEqual(expected, actual)) {
                throw CertificateException("Host certificate fingerprint does not match")
            }
        }

        override fun getAcceptedIssuers(): Array<X509Certificate> = emptyArray()
    }

    companion object {
        private const val MAX_FRAME_BYTES = 64 * 1024
        private const val MAX_COMMIT_BYTES = 512 * 1024
        private const val MAX_COMMIT_CHUNK_BYTES = 36 * 1024
        private const val MAX_PENDING_BYTES = 8L * 1024 * 1024
    }
}

