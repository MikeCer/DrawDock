package com.drawdock

import android.view.MotionEvent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class InputReducerTest {
    private fun sample(
        id: Int,
        tool: Int = MotionEvent.TOOL_TYPE_STYLUS,
    ) = PenSample(id, tool, 10f, 20f, 1_000L, 0.5f, null, null, 0)

    @Test
    fun fingerDoesNotClaimThePenGesture() {
        val reducer = InputReducer()
        assertEquals(
            InputResult.Ignored,
            reducer.handle(MotionEvent.ACTION_DOWN, 3, sample(3, MotionEvent.TOOL_TYPE_FINGER)),
        )
        assertTrue(reducer.handle(MotionEvent.ACTION_POINTER_DOWN, 8, sample(8)) is InputResult.Started)
    }

    @Test
    fun activePointerIdentitySurvivesIndexChangesAndUpEndsIt() {
        val reducer = InputReducer()
        reducer.handle(MotionEvent.ACTION_DOWN, 11, sample(11))
        assertEquals(InputResult.Ignored, reducer.handle(MotionEvent.ACTION_MOVE, 9, sample(9)))
        assertTrue(reducer.handle(MotionEvent.ACTION_MOVE, 11, sample(11)) is InputResult.Moved)
        assertTrue(reducer.handle(MotionEvent.ACTION_POINTER_UP, 11, sample(11)) is InputResult.Ended)
    }

    @Test
    fun cancelAlwaysClearsTheActiveContact() {
        val reducer = InputReducer()
        reducer.handle(MotionEvent.ACTION_DOWN, 11, sample(11))
        val result = reducer.handle(MotionEvent.ACTION_CANCEL, -1, null)
        assertTrue(result is InputResult.Ended && result.canceled)
        assertEquals(InputResult.Ignored, reducer.handle(MotionEvent.ACTION_MOVE, 11, sample(11)))
    }

    @Test
    fun invalidCoordinatesCannotBecomeInkAndInvalidUpCancels() {
        val reducer = InputReducer()
        reducer.handle(MotionEvent.ACTION_DOWN, 11, sample(11))
        val invalid = sample(11).copy(x = Float.NaN)
        assertEquals(InputResult.Ignored, reducer.handle(MotionEvent.ACTION_MOVE, 11, invalid))
        val end = reducer.handle(MotionEvent.ACTION_UP, 11, invalid)
        assertTrue(end is InputResult.Ended && end.canceled && end.sample == null)
    }
}
