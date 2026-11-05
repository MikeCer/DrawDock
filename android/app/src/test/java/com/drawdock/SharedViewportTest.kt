package com.drawdock

import org.junit.Assert.*
import org.junit.Test

class SharedViewportTest {
    @Test
    fun anchoredPinchAndPanAreBoundedAndResetCannotKeepOffsets() {
        val view = SharedViewport().navigate(2f, .4f, .6f, .4f, .6f)
        assertEquals(.2f, view.left, .00001f)
        assertEquals(.3f, view.top, .00001f)
        val anchor = view.source(.4f, .6f)
        assertEquals(.4f, anchor.first, .00001f)
        assertEquals(.6f, anchor.second, .00001f)
        val panned = view.navigate(1f, .5f, .5f, .7f, .2f)
        assertEquals(.1f, panned.left, .00001f)
        assertEquals(.45f, panned.top, .00001f)
        assertEquals(4f, view.navigate(10f, .5f, .5f, .5f, .5f).scale)
        assertEquals(SharedViewport(), view.navigate(.01f, .5f, .5f, .9f, .1f))
    }

    @Test
    fun wireMarkWidthsAndPointsMapBackToCanonicalScene() {
        val viewport = SharedViewport(2f, .2f, .1f)
        val mark = InkMark(DrawTool.FREEHAND, 0xFFFF0000.toInt(), 12f,
            listOf(200f to 200f, 1600f to 900f))
        val mapped = viewport.mark(mark, 2000, 1000)
        assertEquals(6f, mapped.width)
        assertEquals(500f, mapped.points[0].first, .001f)
        assertEquals(200f, mapped.points[0].second, .001f)
        val visible = viewport.matrix.map(mapped.points[1].first / 2000, mapped.points[1].second / 1000)
        assertEquals(.8f, visible.first, .00001f)
        assertEquals(.9f, visible.second, .00001f)
    }

    @Test
    fun selectionViewportCompositionPreservesRotationAndOppositeResizeAnchor() {
        val viewport = SharedViewport(3f, .2f, .1f)
        val local = EditorBounds(.25f, .2f, .4f, .35f)
        val rotation = Affine2D.rotation(.4f, 2000, 1000, local.centerX, local.centerY)
        val selected = EditorSelection(1, local, viewport.matrix * rotation, local)
        val outline = selected.outline()
        val resized = selected.resize(2, outline[2], outline[2].first + .1f to outline[2].second + .05f)
        val canonical = viewport.matrix.inverse() * resized
        val anchor = rotation.map(local.left, local.top)
        val actual = canonical.map(local.left, local.top)
        assertEquals(anchor.first, actual.first, .00001f)
        assertEquals(anchor.second, actual.second, .00001f)
    }

    @Test
    fun twoFingerNavigationUsesPhysicalSpanAndStablePointerIds() {
        val gesture = TwoFingerNavigation(7, 42, SharedViewport(),
            300f to 300f, 500f to 300f, 1000, 500)
        val view = gesture.update(200f to 350f, 600f to 350f)
        assertEquals(2f, view.scale)
        assertEquals(.2f, view.left, .00001f)
        assertEquals(.25f, view.top, .00001f)
        assertEquals(7, gesture.firstId)
        assertEquals(42, gesture.secondId)
    }
}
