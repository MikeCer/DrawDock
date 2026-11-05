package com.drawdock

import org.junit.Assert.*
import org.junit.Test
import kotlin.math.abs

class RadialMenuGeometryTest {
    @Test
    fun cornersKeepEveryButtonInsideTheWorkspaceWithoutOverlap() {
        for (slot in listOf(66, 96, 140)) {
            for ((x, y) in listOf(0f to 0f, 1200f to 0f, 0f to 800f, 1200f to 800f)) {
                val ring = requireNotNull(RadialMenuGeometry.fit(1200, 800, x, y, slot, 350f, 8f))
                for (count in 2..8) {
                    val points = (0 until count).map { ring.itemCenter(it, count) } +
                        (ring.centerX to ring.centerY)
                    for ((px, py) in points) {
                        assertTrue(px - slot / 2f >= 8f)
                        assertTrue(py - slot / 2f >= 8f)
                        assertTrue(px + slot / 2f <= 1192f)
                        assertTrue(py + slot / 2f <= 792f)
                    }
                    for (i in points.indices) for (j in 0 until i) {
                        assertTrue(abs(points[i].first - points[j].first) >= slot ||
                            abs(points[i].second - points[j].second) >= slot)
                    }
                }
            }
        }
    }

    @Test
    fun smallWindowsAndLargeTextUseAListInsteadOfShrinkingTargets() {
        assertNull(RadialMenuGeometry.fit(320, 240, 160f, 120f, 66, 350f, 8f))
        assertNull(RadialMenuGeometry.fit(600, 500, 300f, 250f, 140, 350f, 8f))
        val normal = requireNotNull(RadialMenuGeometry.fit(600, 500, 300f, 250f, 66, 350f, 8f))
        assertEquals(66, normal.slot)
    }

    @Test
    fun firstItemIsAtTheTopAndOrderIsClockwise() {
        val ring = requireNotNull(RadialMenuGeometry.fit(1200, 800, 600f, 400f, 66, 350f, 8f))
        assertEquals(ring.centerX, ring.itemCenter(0, 8).first, 0.001f)
        assertTrue(ring.itemCenter(0, 8).second < ring.centerY)
        assertTrue(ring.itemCenter(1, 8).first > ring.centerX)
        assertEquals(ring.centerY, ring.itemCenter(2, 8).second, 0.001f)
    }
}
