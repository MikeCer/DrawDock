package com.drawdock

import org.junit.Assert.*
import org.junit.Test

class VideoHandoffTest {
    private val mark = InkMark(DrawTool.HIGHLIGHTER, 0x59FFCC00, .004f,
        listOf(.2f to .3f, .4f to .5f))
    private fun frame(id: Long, epoch: Long, revision: Long?) =
        VideoFrameState(id, epoch, revision, revision?.let { SharedViewport() })

    @Test fun reconnectCannotMistakeALateOldTextureForTheNewFirstFrame() {
        val clock = VideoTimestampAllocator()
        val oldPts = clock.next(33_333)
        val nextPts = clock.next(33_333)
        val replacement = VideoFrameTimeline()
        replacement.queue(nextPts, frame(1, 1, 0))
        assertNull(replacement.latch(oldPts * 1000))
        assertEquals(1L, replacement.latch(nextPts * 1000)?.id)
        assertTrue(clock.next(33_333) > nextPts)
    }

    @Test fun acknowledgementNeverRemovesInkBeforeTheMatchingTextureLatches() {
        val pending = PendingInk()
        val timeline = VideoFrameTimeline()
        pending.add(7, 1, mark, 0)
        pending.acknowledge(7, 1, 4)
        val committed = frame(3, 1, 4)
        timeline.queue(100, committed)
        assertTrue(timeline.presentable(100))
        assertEquals(listOf(mark), pending.marks)
        assertNull(timeline.latch(99_000))
        pending.frameLatched(frame(2, 1, 3))
        assertEquals(listOf(mark), pending.marks)
        pending.frameLatched(checkNotNull(timeline.latch(100_000)))
        assertTrue(pending.isEmpty)
        assertNull(timeline.latch(100_000))
    }

    @Test fun lateAcknowledgementsAndZoomEpochsRetireOnlyTheirDisplayedCommits() {
        val pending = PendingInk()
        pending.add(7, 1, mark, 0)
        pending.add(8, 1, mark, 10)
        pending.frameLatched(frame(3, 2, 4))
        assertEquals(2, pending.marks.size)
        pending.acknowledge(7, 1, 4)
        assertEquals(1, pending.marks.size)
        pending.acknowledge(8, 1, 5)
        pending.frameLatched(frame(4, 2, null))
        assertEquals(1, pending.marks.size)
        pending.frameLatched(frame(5, 2, 5))
        assertTrue(pending.isEmpty)
    }

    @Test fun rejectionBudgetsAndSessionResetDoNotLeakOrResurrectPreviews() {
        val pending = PendingInk()
        repeat(128) { pending.add(it.toLong(), 1, mark, it.toLong()) }
        assertFalse(pending.canAdd(mark))
        pending.reject(0)
        assertEquals(1L, pending.oldestSubmission)
        assertTrue(pending.canAdd(mark))
        pending.clear()
        pending.acknowledge(1, 1, 2)
        pending.frameLatched(frame(100, 4, 99))
        assertTrue(pending.isEmpty)
        val large = mark.copy(points = List(20_000) { .1f to .2f })
        repeat(6) { pending.add(it.toLong(), 4, large, 0) }
        assertFalse(pending.canAdd(large))
        repeat(6) { pending.reject(it.toLong()) }
        assertTrue(pending.canAdd(large))
    }

    @Test fun unknownAndSkippedVideoFramesCannotProduceFalseWatermarks() {
        val timeline = VideoFrameTimeline()
        timeline.queue(100, frame(1, 0, null))
        assertFalse(timeline.presentable(100))
        timeline.discard(100)
        assertNull(timeline.latch(100_000))
        timeline.queue(200, frame(2, 1, 2))
        timeline.queue(300, frame(3, 1, 3))
        assertEquals(3L, timeline.latch(300_000)?.revision)
        assertNull(timeline.latch(200_000))
        timeline.queue(400, frame(4, 2, 4))
        timeline.clear()
        assertNull(timeline.latch(400_000))
    }
}
