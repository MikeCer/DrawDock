package com.drawdock

import org.junit.Assert.assertEquals
import org.junit.Test

class InkMarkTest {
    @Test
    fun shapesCommitOnlyDragEndpointsInEitherDirection() {
        val samples = listOf(90f to 80f, 60f to 50f, 30f to 20f, 10f to 5f)
        for (tool in listOf(DrawTool.ARROW, DrawTool.RECTANGLE, DrawTool.ELLIPSE)) {
            for (points in listOf(samples, samples.reversed())) {
                val mark = InkMark(tool, 0, 6f, points)
                assertEquals(listOf(points.first(), points.last()), mark.commitPoints())
            }
        }
    }

    @Test
    fun freehandPreservesEverySample() {
        val samples = listOf(10f to 20f, 30f to 50f, 40f to 10f)
        assertEquals(samples, InkMark(DrawTool.FREEHAND, 0, 6f, samples).commitPoints())
    }
}
