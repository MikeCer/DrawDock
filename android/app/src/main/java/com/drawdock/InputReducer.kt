package com.drawdock

import android.view.MotionEvent
enum class DrawTool {
    FREEHAND,
    ARROW,
    RECTANGLE,
    ELLIPSE,
    CIRCLE,
    LINE,
    POLYGON,
    POLYLINE,
    TEXT,
    IMAGE,
    HIGHLIGHTER,
    LASER,
    SELECT_MOVE,
    ROTATE,
    RESIZE,
}

data class PenSample(
    val pointerId: Int,
    val toolType: Int,
    val x: Float,
    val y: Float,
    val eventTimeUs: Long,
    val pressure: Float?,
    val tilt: Float?,
    val orientation: Float?,
    val buttonState: Int,
)

sealed interface InputResult {
    data class Started(val sample: PenSample) : InputResult
    data class Moved(val sample: PenSample) : InputResult
    data class Ended(val sample: PenSample?, val canceled: Boolean) : InputResult
    data object Ignored : InputResult
}

class InputReducer {
    private var activePointerId: Int? = null
    val currentPointerId: Int? get() = activePointerId

    fun handle(
        action: Int,
        pointerId: Int,
        sample: PenSample?,
        canceled: Boolean = false,
    ): InputResult {
        if (action == MotionEvent.ACTION_CANCEL) {
            val active = activePointerId ?: return InputResult.Ignored
            activePointerId = null
            return InputResult.Ended(sample?.takeIf { it.pointerId == active }, canceled = true)
        }

        if (action == MotionEvent.ACTION_DOWN || action == MotionEvent.ACTION_POINTER_DOWN) {
            if (activePointerId != null || sample == null || !sample.isPenLike()) {
                return InputResult.Ignored
            }
            activePointerId = pointerId
            return InputResult.Started(sample)
        }

        if (activePointerId != pointerId) return InputResult.Ignored

        if (action == MotionEvent.ACTION_MOVE) {
            return sample?.takeIf { it.isPenLike() }?.let(InputResult::Moved) ?: InputResult.Ignored
        }

        if (action == MotionEvent.ACTION_UP || action == MotionEvent.ACTION_POINTER_UP) {
            activePointerId = null
            return InputResult.Ended(sample?.takeIf { it.isPenLike() }, canceled || sample?.isPenLike() != true)
        }
        return InputResult.Ignored
    }

    fun cancel() {
        activePointerId = null
    }

}

private fun PenSample.isPenLike(): Boolean =
    toolType == MotionEvent.TOOL_TYPE_STYLUS && x.isFinite() && y.isFinite()
