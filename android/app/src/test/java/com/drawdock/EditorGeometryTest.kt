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

    @Test
    fun acknowledgedRotationKeepsOrientedCornersRatherThanRotatingTheWorldBounds() {
        val local = EditorBounds(0.35f, 0.35f, 0.65f, 0.65f)
        val rotation = Affine2D.rotation(Math.PI.toFloat() / 4, 2000, 1000, 0.5f, 0.5f)
        val selected = EditorSelection(1, local, Affine2D(), local)
        val preview = selected.outline(rotation)
        val acknowledged = selected.copy(
            bounds = EditorBounds(0.340901f, 0.181802f, 0.659099f, 0.818198f),
            transform = rotation,
        )
        assertPoint(0.446967f to 0.181802f, preview[0])
        assertPoint(0.659099f to 0.606066f, preview[1])
        preview.zip(acknowledged.outline()).forEach { (before, after) -> assertPoint(before, after) }
        assertPoint(0.5f to 0.5f, acknowledged.center)
    }

    @Test
    fun everyResizeCornerKeepsItsOppositeFixedAfterRotationMirrorAndShear() {
        val local = EditorBounds(0.2f, 0.3f, 0.6f, 0.7f)
        val matrix = Affine2D.rotation(0.6f, 2000, 1000, 0.4f, 0.5f) *
            Affine2D.around(0.4f, 0.5f, -1.2f, 0f, 0.2f, 0.8f)
        val selected = EditorSelection(1, local, matrix, local)
        val localCorners = selected.copy(transform = Affine2D()).outline()
        for (corner in 0..3) {
            val handle = localCorners[corner]
            val anchor = localCorners[(corner + 2) % 4]
            // A pen can land within the large hit target rather than exactly on the icon.
            val start = matrix.map(handle.first + 0.005f, handle.second - 0.004f)
            val end = matrix.map(
                handle.first + 0.005f + (handle.first - anchor.first) * 0.5f,
                handle.second - 0.004f + (handle.second - anchor.second) * -0.35f,
            )
            val resized = selected.resize(corner, start, end)
            val expected = matrix * Affine2D.around(anchor.first, anchor.second, 1.5f, 0f, 0f, 0.65f)
            for ((x, y) in localCorners) assertPoint(expected.map(x, y), resized.map(x, y))
            assertPoint(matrix.map(anchor.first, anchor.second), resized.map(anchor.first, anchor.second))
            val second = selected.copy(transform = resized)
            assertPoint(second.outline()[corner],
                second.resize(corner, end, end).map(handle.first, handle.second))
        }
    }

    @Test
    fun resizingSmallObjectsStillTracksThePenAndFixesTheOppositeCorner() {
        val local = EditorBounds(0.5f, 0.5f, 0.505f, 0.507f)
        val selected = EditorSelection(1, local, Affine2D(), local)
        val resized = selected.resize(2, 0.505f to 0.507f, 0.508f to 0.511f)
        assertPoint(0.508f to 0.511f, resized.map(0.505f, 0.507f))
        assertPoint(0.5f to 0.5f, resized.map(0.5f, 0.5f))
    }

    @Test
    fun penHitTargetsFollowTheRotatedFrameAndDoNotCaptureItsEmptyBoundingBox() {
        val local = EditorBounds(0.3f, 0.3f, 0.7f, 0.7f)
        val selected = EditorSelection(1, local,
            Affine2D.rotation(Math.PI.toFloat() / 4, 1000, 1000, 0.5f, 0.5f), local)
        val frame = selected.frame(1000, 1000, 48f, 16f)
        val corner = frame.corners[2]
        assertEquals(2, frame.hitHandle(corner.first + 23f, corner.second, 24f))
        assertNull(frame.hitHandle(corner.first + 25f, corner.second, 24f))
        assertTrue(frame.contains(500f, 500f, 12f))
        assertFalse(frame.contains(230f, 230f, 12f))
        assertEquals(-1, frame.hitHandle(frame.rotationHandle.first, frame.rotationHandle.second, 24f))
    }

    @Test
    fun rotationHandleStaysOutsideTheTopEdgeEvenWhenMirroredAndIsClampedAtScreenEdges() {
        val local = EditorBounds(0.3f, 0.3f, 0.7f, 0.7f)
        for (angle in listOf(0f, 0.7f, 2f)) for (mirror in listOf(-1f, 1f)) {
            val selected = EditorSelection(1, local,
                Affine2D.rotation(angle, 1000, 1000, 0.5f, 0.5f) *
                    Affine2D.around(0.5f, 0.5f, mirror, 0f, 0f, 1f), local)
            val frame = selected.frame(1000, 1000, 48f, 16f)
            val top = (frame.corners[0].first + frame.corners[1].first) / 2 to
                (frame.corners[0].second + frame.corners[1].second) / 2
            val dx = frame.rotationHandle.first - top.first
            val dy = frame.rotationHandle.second - top.second
            assertEquals(48f, kotlin.math.hypot(dx, dy), 0.0001f)
            assertTrue(dx * (top.first - 500) + dy * (top.second - 500) > 0)
        }
        val nearEdge = EditorSelection(1, local, Affine2D(ty = -0.3f), local)
        assertEquals(16f, nearEdge.frame(1000, 1000, 48f, 16f).rotationHandle.second, 0.0001f)
    }

    @Test
    fun zeroSizeSelectionDoesNotCaptureTheWholeScreen() {
        val point = EditorBounds(0.5f, 0.5f, 0.5f, 0.5f)
        val frame = EditorSelection(1, point, Affine2D(), point).frame(1000, 1000, 48f, 16f)
        assertFalse(frame.contains(10f, 10f, 12f))
        assertTrue(frame.contains(500f, 500f, 12f))
        assertNull(frame.hitHandle(500f, 500f, 24f, keepMoveCenter = true))
        assertNotNull(frame.hitHandle(500f, 500f, 24f))
        assertEquals(-1, frame.hitHandle(500f, 452f, 24f, keepMoveCenter = true))
    }

    private fun assertPoint(expected: Pair<Float, Float>, actual: Pair<Float, Float>) {
        assertEquals(expected.first, actual.first, 0.00001f)
        assertEquals(expected.second, actual.second, 0.00001f)
    }
}
