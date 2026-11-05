package com.drawdock

import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaCodecList
import android.media.MediaFormat
import android.util.Log
import android.view.Surface
import org.json.JSONObject
import java.io.DataInputStream
import java.io.IOException
import java.io.InputStream
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference
import kotlin.concurrent.thread

internal class RemoteVideoDecoder(
    private val surface: Surface,
    private val onGeometry: (Long, Int, Int) -> Unit,
) {
    companion object {
        private val presentationClock = VideoTimestampAllocator()
    }
    private val timeline = VideoFrameTimeline()
    private var latched = false

    fun frameLatched(timestampNs: Long): VideoFrameState? = timeline.latch(timestampNs)?.also {
        if (!latched) {
            Log.i("DrawDockVideo", "First texture-latched frame ${it.id}, epoch ${it.epoch}")
            latched = true
        }
    }

    fun run(input: InputStream) {
        val stream = DataInputStream(input)
        var codec: MediaCodec? = null
        var outputThread: Thread? = null
        val decoding = AtomicBoolean(false)
        val outputFailure = AtomicReference<Exception?>(null)
        var configEpoch = -1L
        var lastFrameId = 0L
        var pendingFrame: VideoFrameState? = null
        var framesPerSecond = 30

        fun stopDecoder() {
            decoding.set(false)
            val worker = outputThread
            outputThread = null
            worker?.join(1_000)
            val decoder = codec
            codec = null
            try {
                decoder?.release()
            } finally {
                timeline.clear()
            }
            check(worker?.isAlive != true) { "Video output thread did not stop; reconnect required" }
        }

        try {
            while (true) {
                outputFailure.get()?.let { throw it }
                val kind = stream.readUnsignedByte()
                val packetEpoch = stream.readLong()
                val length = stream.readInt()
                require(length in 1..8 * 1024 * 1024) { "Invalid video packet length" }
                require(kind == 2 || length <= 64 * 1024) { "Video metadata is too large" }
                val bytes = ByteArray(length)
                stream.readFully(bytes)
                when (kind) {
                    1 -> {
                        require(pendingFrame == null) { "Missing video frame after metadata" }
                        val config = JSONObject(String(bytes, Charsets.UTF_8))
                        require(config.getString("codec") == "video/avc" &&
                            config.getString("annotation_capture_mode") == "included" &&
                            config.getInt("frame_metadata") == 1) { "Incompatible PC video protocol; update both apps" }
                        val geometry = VideoGeometry(config.getInt("width"), config.getInt("height"), config.getInt("fps"))
                        framesPerSecond = geometry.fps
                        require(packetEpoch > configEpoch) { "Invalid video configuration epoch" }
                        stopDecoder()
                        val format = MediaFormat.createVideoFormat("video/avc", geometry.width, geometry.height)
                        format.setInteger(MediaFormat.KEY_FRAME_RATE, geometry.fps)
                        format.setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, 8 * 1024 * 1024)
                        format.setInteger(MediaFormat.KEY_PRIORITY, 0)
                        val decoderName = checkNotNull(MediaCodecList(MediaCodecList.REGULAR_CODECS).findDecoderForFormat(format)) {
                            "No AVC decoder supports ${geometry.width}x${geometry.height} at ${geometry.fps} fps"
                        }
                        val decoder = MediaCodec.createByCodecName(decoderName)
                        codec = decoder
                        val capabilities = decoder.codecInfo.getCapabilitiesForType("video/avc")
                        check(capabilities.videoCapabilities?.areSizeAndRateSupported(
                            geometry.width, geometry.height, geometry.fps.toDouble()) == true) {
                            "Tablet AVC decoder does not support selected geometry"
                        }
                        if (capabilities.isFeatureSupported(MediaCodecInfo.CodecCapabilities.FEATURE_LowLatency)) {
                            format.setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
                        }
                        decoder.configure(format, surface, null, 0)
                        decoder.start()
                        configEpoch = packetEpoch
                        decoding.set(true)
                        outputThread = thread(name = "drawdock-video-output", isDaemon = true) {
                            val info = MediaCodec.BufferInfo()
                            try {
                                while (decoding.get()) {
                                    val index = decoder.dequeueOutputBuffer(info, 10_000)
                                    if (index >= 0) {
                                        val present = timeline.presentable(info.presentationTimeUs)
                                        decoder.releaseOutputBuffer(index, present)
                                        if (!present) timeline.discard(info.presentationTimeUs)
                                    }
                                }
                            } catch (error: Exception) {
                                if (decoding.get()) {
                                    outputFailure.set(error)
                                    try { input.close() } catch (closeError: IOException) {
                                        error.addSuppressed(closeError)
                                    }
                                }
                            }
                        }
                        Log.i("DrawDockVideo", "Configured ${geometry.width}x${geometry.height} epoch $configEpoch")
                        onGeometry(configEpoch, geometry.width, geometry.height)
                    }
                    4 -> {
                        check(codec != null && pendingFrame == null) { "Unexpected video frame metadata" }
                        val metadata = JSONObject(String(bytes, Charsets.UTF_8))
                        val id = metadata.getLong("frame_id")
                        require(id > lastFrameId && id <= Long.MAX_VALUE / 1_000_000) { "Invalid video frame sequence" }
                        val presentation = metadata.optJSONObject("presentation")
                        val revision = presentation?.getLong("revision")
                        val zoom = presentation?.getJSONObject("zoom")
                        require(if (presentation == null) packetEpoch == 0L else
                            packetEpoch > 0 && presentation.getLong("epoch") == packetEpoch && revision != null && revision >= 0) {
                            "Invalid presented-scene watermark"
                        }
                        val viewport = zoom?.let {
                            SharedViewport(it.getDouble("scale").toFloat(), it.getDouble("left").toFloat(),
                                it.getDouble("top").toFloat())
                        }
                        pendingFrame = VideoFrameState(id, packetEpoch, revision, viewport)
                        lastFrameId = id
                    }
                    2 -> {
                        val frame = checkNotNull(pendingFrame) { "Video frame arrived without metadata" }
                        pendingFrame = null
                        require(packetEpoch == frame.epoch) { "Video frame does not match its metadata" }
                        val decoder = checkNotNull(codec) { "Video frame arrived before configuration" }
                        var index = decoder.dequeueInputBuffer(20_000)
                        if (index < 0) index = decoder.dequeueInputBuffer(100_000)
                        check(index >= 0) { "Video decoder stalled; reconnect required" }
                        val buffer = checkNotNull(decoder.getInputBuffer(index))
                        require(bytes.size <= buffer.capacity()) { "H.264 frame exceeds decoder capacity" }
                        buffer.clear()
                        buffer.put(bytes)
                        // The TextureView outlives sessions; never reuse an old surface timestamp.
                        val ptsUs = presentationClock.next(1_000_000L / framesPerSecond)
                        timeline.queue(ptsUs, frame)
                        decoder.queueInputBuffer(index, 0, bytes.size, ptsUs, 0)
                    }
                    3 -> throw IllegalStateException(String(bytes, Charsets.UTF_8))
                    else -> throw IllegalArgumentException("Unknown video packet type")
                }
            }
        } catch (error: Exception) {
            throw outputFailure.get() ?: error
        } finally {
            stopDecoder()
        }
    }
}
