package com.drawdock

import org.junit.Assert.*
import org.junit.Test

class SessionOutboxTest {
    @Test
    fun laserMotionCoalescesWithoutLosingStopSourceOrControlBarriers() {
        val outbox = SessionOutbox()
        fun laser(x: Float, active: Boolean = true, epoch: Long = 1) =
            SessionCommand.Laser(active, x, 0.5f, 0, epoch)
        repeat(2_000) { assertTrue(outbox.offer(laser(it.toFloat()))) }
        val stop = laser(1999f, false)
        assertTrue(outbox.offer(stop))
        val next = laser(0f)
        assertTrue(outbox.offer(next))
        val newSource = laser(0f, epoch = 2)
        assertTrue(outbox.offer(newSource))
        assertEquals(laser(1999f), outbox.poll(0))
        assertEquals(stop, outbox.poll(0))
        assertEquals(next, outbox.poll(0))
        assertEquals(newSource, outbox.poll(0))
        assertNull(outbox.poll(0))
    }

    @Test
    fun sourceEpochChangePreventsCoalescingOldAndNewGeometry() {
        val outbox = SessionOutbox()
        val old = pointer("hover").copy(sourceEpoch = 1)
        val new = pointer("hover").copy(sourceEpoch = 2)
        assertTrue(outbox.offer(old))
        assertTrue(outbox.offer(new))
        assertEquals(old, outbox.poll(0))
        assertEquals(new, outbox.poll(0))
    }

    private fun pointer(action: String, x: Float = 0f, generation: Long = 1) =
        SessionCommand.Pointer(action, generation, x, 20f, "left", 100, 100)

    @Test
    fun fastWriterCoalescesTwoThousandHoverSamplesPerSecondBelowHostLimit() {
        val outbox = SessionOutbox()
        val sent = mutableListOf<Long>()
        for (sample in 0 until 10_000) {
            val now = sample * 500_000L
            val latest = pointer("hover", sample.toFloat(), generation = 0)
            assertTrue(outbox.offer(latest))
            if (outbox.delayBeforeFrame(now) == 0L) {
                assertEquals(latest, outbox.poll(0))
                outbox.frameWritten(now)
                sent.add(now)
            }
        }
        for (second in 0 until 5) {
            val count = sent.count { it / 1_000_000_000L == second.toLong() }
            assertTrue("Unexpected frame rate: $count", count in 60..80)
        }
        assertEquals(9_999f, (outbox.poll(0) as SessionCommand.Pointer).x)
        assertNull(outbox.poll(0))
    }

    @Test
    fun motionCoalescingPreservesClickReleaseCancelModeAndGenerationBarriers() {
        val outbox = SessionOutbox()
        val down = pointer("down")
        val up = pointer("up", 30f)
        val secondDown = pointer("down", generation = 2)
        val cancel = pointer("cancel", generation = 2)
        val mode = SessionCommand.Mode("draw", 1)
        assertTrue(outbox.offer(down))
        repeat(1_000) { assertTrue(outbox.offer(pointer("move", it.toFloat()))) }
        assertTrue(outbox.offer(up))
        assertTrue(outbox.offer(secondDown))
        assertTrue(outbox.offer(pointer("move", 40f, generation = 2)))
        assertTrue(outbox.offer(cancel))
        assertTrue(outbox.offer(mode))
        assertTrue(outbox.offer(pointer("hover", generation = 2)))
        val expected = listOf(
            down, pointer("move", 999f), up, secondDown,
            pointer("move", 40f, generation = 2), cancel, mode,
            pointer("hover", generation = 2),
        )
        for (command in expected) assertEquals(command, outbox.poll(0))
        assertNull(outbox.poll(0))
    }

    @Test
    fun fullQueueReservesAReleaseSlotAndReportsOtherOverflow() {
        for (release in listOf("up", "cancel")) {
            val outbox = SessionOutbox()
            repeat(128) { assertTrue(outbox.offer(SessionCommand.Control("undo"))) }
            assertFalse(outbox.offer(SessionCommand.ClearDisplay(1)))
            assertTrue(outbox.offer(pointer(release)))
            assertFalse(outbox.offer(pointer(release)))
            repeat(128) { assertEquals(SessionCommand.Control("undo"), outbox.poll(0)) }
            assertEquals(pointer(release), outbox.poll(0))
            assertNull(outbox.poll(0))
        }
    }

    @Test
    fun sendStallsDoNotAccumulateCreditForCatchUpBursts() {
        val outbox = SessionOutbox()
        outbox.frameWritten(0)
        assertTrue(outbox.delayBeforeFrame(1) > 0)
        val afterStall = 5_000_000_000L
        assertEquals(0, outbox.delayBeforeFrame(afterStall))
        outbox.frameWritten(afterStall)
        assertTrue(outbox.delayBeforeFrame(afterStall + 1) > 0)
        assertEquals(0, outbox.delayBeforeFrame(afterStall + 20_000_000))
    }
}
