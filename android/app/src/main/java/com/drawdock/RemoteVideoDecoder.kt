package com.drawdock

import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaCodecList
import android.media.MediaFormat
import android.os.Handler
import android.os.Looper
import android.view.Surface
import org.json.JSONObject
import java.io.DataInputStream
import java.io.InputStream

internal class RemoteVideoDecoder(
    private val surface: Surface,
    private val onGeometry: (Long, Int, Int) -> Unit,
    private val onRendered: (Long) -> Unit,
) {
    fun run(input: InputStream) {
        val stream = DataInputStream(input)
        var codec: MediaCodec? = null
        var epoch = -1L
        var frameNumber = 0L
        var framesPerSecond = 30
        val info = MediaCodec.BufferInfo()
        try {
            while (true) {
                val kind = stream.readUnsignedByte()
                val packetEpoch = stream.readLong()
                val length = stream.readInt()
                require(length in 1..8 * 1024 * 1024) { "Invalid video packet length" }
                require(kind == 2 || length <= 64 * 1024) { "Video metadata is too large" }
                val bytes = ByteArray(length)
                stream.readFully(bytes)
                when (kind) {
                    1 -> {
                        val config = JSONObject(String(bytes, Charsets.UTF_8))
                        require(config.getString("codec") == "video/avc") { "Unsupported video codec" }
                        require(config.getString("annotation_capture_mode") == "included") {
                            "Unsupported annotation capture mode"
                        }
                        val geometry = VideoGeometry(config.getInt("width"), config.getInt("height"), config.getInt("fps"))
                        val width = geometry.width
                        val height = geometry.height
                        framesPerSecond = geometry.fps
                        require(packetEpoch > epoch) { "Invalid video epoch" }
                        codec?.stop()
                        codec?.release()
                        codec = null
                        val format = MediaFormat.createVideoFormat("video/avc", width, height)
                        format.setInteger(MediaFormat.KEY_FRAME_RATE, framesPerSecond)
                        format.setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, 8 * 1024 * 1024)
                        format.setInteger(MediaFormat.KEY_PRIORITY, 0)
                        val decoderName = checkNotNull(MediaCodecList(MediaCodecList.REGULAR_CODECS).findDecoderForFormat(format)) {
                            "No AVC decoder supports ${width}x$height at $framesPerSecond fps"
                        }
                        val decoder = MediaCodec.createByCodecName(decoderName)
                        codec = decoder
                        val capabilities = decoder.codecInfo.getCapabilitiesForType("video/avc")
                        check(capabilities.videoCapabilities?.areSizeAndRateSupported(width, height, framesPerSecond.toDouble()) == true) {
                            "Tablet AVC decoder does not support ${width}x$height at $framesPerSecond fps"
                        }
                        if (capabilities.isFeatureSupported(MediaCodecInfo.CodecCapabilities.FEATURE_LowLatency)
                        ) {
                            format.setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
                        }
                        decoder.configure(format, surface, null, 0)
                        val configuredEpoch = packetEpoch
                        decoder.setOnFrameRenderedListener(
                            { _, _, _ -> onRendered(configuredEpoch) },
                            Handler(Looper.getMainLooper()),
                        )
                        decoder.start()
                        epoch = packetEpoch
                        frameNumber = 0
                        onGeometry(epoch, width, height)
                    }
                    2 -> {
                        require(packetEpoch == epoch) { "Frame does not match configured epoch" }
                        val decoder = checkNotNull(codec) { "Video frame arrived before configuration" }
                        var index = decoder.dequeueInputBuffer(20_000)
                        if (index < 0) {
                            drain(decoder, info)
                            index = decoder.dequeueInputBuffer(100_000)
                        }
                        check(index >= 0) { "Video decoder stalled; reconnect required" }
                        val buffer = checkNotNull(decoder.getInputBuffer(index))
                        require(bytes.size <= buffer.capacity()) { "H.264 frame exceeds decoder capacity" }
                        buffer.clear()
                        buffer.put(bytes)
                        decoder.queueInputBuffer(index, 0, bytes.size, frameNumber++ * 1_000_000 / framesPerSecond, 0)
                        drain(decoder, info)
                    }
                    3 -> throw IllegalStateException(String(bytes, Charsets.UTF_8))
                    else -> throw IllegalArgumentException("Unknown video packet type")
                }
            }
        } finally {
            codec?.release()
        }
    }

    private fun drain(codec: MediaCodec, info: MediaCodec.BufferInfo) {
        while (true) {
            val index = codec.dequeueOutputBuffer(info, 0)
            when {
                index >= 0 -> codec.releaseOutputBuffer(index, true)
                index == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> continue
                else -> return
            }
        }
    }
}
