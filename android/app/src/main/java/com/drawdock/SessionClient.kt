package com.drawdock

import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
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
) {
    private val mainHandler = Handler(Looper.getMainLooper())
    private val closed = AtomicBoolean(false)
    private val commands = SessionOutbox()
    private val queuedBytes = AtomicLong(0)
    private val nextObjectId = AtomicLong(SystemClock.elapsedRealtimeNanos())
    private val nextModeRequestId = AtomicLong(0)
    private val nextClearRequestId = AtomicLong(0)
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
                connection.soTimeout = 10_000
                connection.startHandshake()

                val streamIn = BufferedInputStream(connection.inputStream)
                val streamOut = BufferedOutputStream(connection.outputStream)
                writeFrame(
                    streamOut,
                    JSONObject()
                        .put("type", "pair")
                        .put("token", invitationToken)
                        .put("client", Build.MODEL.take(64)),
                )
                connection.soTimeout = 300_000
                val response = readFrame(streamIn)
                if (response.optString("type") != "paired") {
                    throw SecurityException("Host did not approve this tablet session")
                }

                connection.soTimeout = 0
                output = streamOut
                postStatus("Connected (TLS certificate pinned)", true)
                thread(name = "drawdock-session-writer", isDaemon = true) { writeLoop() }
                thread(name = "drawdock-session-reader", isDaemon = true) {
                    readLoop(streamIn)
                }
            } catch (error: Exception) {
                close()
                postStatus("Connection failed: ${error.message ?: error.javaClass.simpleName}", false)
            }
        }
    }

    fun sendMark(mark: InkMark, viewportWidth: Int, viewportHeight: Int): Boolean {
        if (viewportWidth <= 0 || viewportHeight <= 0 || mark.points.isEmpty()) return false
        val reserved = mark.points.size.toLong() * 8L + 256L
        while (true) {
            val current = queuedBytes.get()
            if (current + reserved > MAX_PENDING_BYTES) {
                postStatus("Pending stroke budget is full; stroke was not sent", socket != null)
                return false
            }
            if (queuedBytes.compareAndSet(current, current + reserved)) break
        }
        val command = SessionCommand.Commit(mark, viewportWidth, viewportHeight, reserved)
        if (!enqueue(command)) {
            queuedBytes.addAndGet(-reserved)
            return false
        }
        return true
    }

    fun undo() = enqueue(SessionCommand.Control("undo"))
    fun redo() = enqueue(SessionCommand.Control("redo"))
    fun clearDisplay(): Long? {
        val requestId = nextClearRequestId.incrementAndGet()
        return requestId.takeIf {
            enqueue(SessionCommand.ClearDisplay(requestId))
        }
    }

    fun requestMode(mode: String): Long? {
        if (mode != "draw" && mode != "pointer") return null
        val requestId = nextModeRequestId.incrementAndGet()
        return requestId.takeIf {
            enqueue(SessionCommand.Mode(mode, requestId))
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
                event.x,
                event.y,
                event.button,
                viewportWidth,
                viewportHeight,
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
                    "error" -> postStatus(
                        "Host rejected command: ${response.optString("message")}",
                        true,
                    )
                    "mode_ack" -> {
                        val mode = response.optString("mode")
                        val requestId = response.optLong("request_id", -1)
                        if (mode in listOf("draw", "pointer") && requestId > 0) {
                            mainHandler.post { onModeAck(mode, requestId) }
                        }
                    }
                    "clear_ack" -> {
                        val requestId = response.optLong("request_id", -1)
                        if (requestId > 0) mainHandler.post { onClearAck(requestId) }
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
        if (!closed.compareAndSet(false, true)) return
        try {
            socket?.close()
        } catch (_: IOException) {
            // Closing a failed TLS socket is best-effort; the original failure is reported.
        }
        socket = null
        output = null
        commands.clear()
        queuedBytes.set(0)
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

