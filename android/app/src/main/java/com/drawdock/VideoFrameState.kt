package com.drawdock

import java.util.TreeMap
import java.util.concurrent.atomic.AtomicLong

internal class VideoTimestampAllocator {
    private val last = AtomicLong(0)

    fun next(intervalUs: Long): Long = last.updateAndGet { previous ->
        require(intervalUs > 0 && previous <= Long.MAX_VALUE / 1000 - intervalUs) {
            "Video presentation clock exhausted; reconnect required"
        }
        previous + intervalUs
    }
}

data class VideoFrameState(
    val id: Long,
    val epoch: Long,
    val revision: Long?,
    val viewport: SharedViewport?,
)

internal class VideoFrameTimeline {
    private val frames = TreeMap<Long, VideoFrameState>()

    @Synchronized fun queue(ptsUs: Long, frame: VideoFrameState) {
        check(frames.size < 128) { "Video surface stopped latching frames; reconnect required" }
        check(frames.put(ptsUs, frame) == null) { "Duplicate video presentation timestamp" }
    }

    @Synchronized fun presentable(ptsUs: Long) = frames[ptsUs]?.revision != null

    @Synchronized fun discard(ptsUs: Long) { frames.remove(ptsUs) }
    @Synchronized fun clear() { frames.clear() }

    @Synchronized fun latch(timestampNs: Long): VideoFrameState? {
        if (timestampNs % 1000L != 0L) return null
        val ptsUs = timestampNs / 1000L
        val frame = frames[ptsUs] ?: return null
        frames.headMap(ptsUs, true).clear()
        return frame
    }
}
