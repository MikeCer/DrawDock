package com.drawdock

import org.junit.Assert.*
import org.junit.Test

class EditorGeometryTest {
    @Test
    fun stationaryHoldWaitsForDeadlineAndFiresOnlyOnce() {
        val gesture = PenHoldGesture(8f)
        gesture.begin(100f, 200f, 1_000)
        gesture.move(103f, 201f)
        assertFalse(gesture.takeLongPress(1_549))
        assertTrue(gesture.pending)
        assertTrue(gesture.takeLongPress(1_550))
        assertFalse(gesture.takeLongPress(1_600))
    }

    @Test
    fun movingBeyondSlopCannotRearmByReturningToOrigin() {
        val gesture = PenHoldGesture(8f)
        gesture.begin(100f, 200f, 0)
        gesture.move(109f, 200f)
        gesture.move(100f, 200f)
        assertFalse(gesture.pending)
        assertFalse(gesture.takeLongPress(1_000))
    }

    @Test
    fun cancellationAndNextContactReplaceTheOldHoldDeadline() {
        val gesture = PenHoldGesture(8f)
        gesture.begin(100f, 200f, 0)
        gesture.cancel()
        assertFalse(gesture.takeLongPress(1_000))
        gesture.begin(300f, 400f, 1_000)
        assertFalse(gesture.takeLongPress(1_001))
        assertTrue(gesture.takeLongPress(1_550))
        assertEquals(300f to 400f, gesture.origin)
    }

    @Test
    fun rotationUsesPhysicalPixelsRatherThanStretchingOnWideMonitors() {
        val rotation = Affine2D.rotation(Math.PI.toFloat() / 2, 2000, 1000, 0.5f, 0.5f)
        val point = rotation.map(0.6f, 0.5f)
        assertEquals(0.5f, point.first, 0.00001f)
        assertEquals(0.7f, point.second, 0.00001f)
        val center = rotation.map(0.5f, 0.5f)
        assertEquals(0.5f, center.first, 0.00001f)
        assertEquals(0.5f, center.second, 0.00001f)
    }

    @Test
    fun mirrorCompositionPreservesPriorMoveAndIsReversible() {
        val moved = Affine2D(tx = 0.2f, ty = 0.1f)
        val mirror = Affine2D.around(0.5f, 0.4f, -1f, 0f, 0f, 1f)
        val point = (mirror * moved).map(0.1f, 0.2f)
        assertEquals(0.7f, point.first, 0.00001f)
        assertEquals(0.3f, point.second, 0.00001f)
        val restored = (mirror * mirror * moved).map(0.1f, 0.2f)
        assertEquals(0.3f, restored.first, 0.00001f)
        assertEquals(0.3f, restored.second, 0.00001f)
    }
}
