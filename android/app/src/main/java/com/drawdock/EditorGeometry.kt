package com.drawdock

import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.sin

data class EditorBounds(val left: Float, val top: Float, val right: Float, val bottom: Float) {
    val centerX get() = (left + right) / 2
    val centerY get() = (top + bottom) / 2
    fun contains(x: Float, y: Float) = x in left..right && y in top..bottom
}

data class Affine2D(
    val a: Float = 1f, val b: Float = 0f, val c: Float = 0f,
    val d: Float = 1f, val tx: Float = 0f, val ty: Float = 0f,
) {
    operator fun times(other: Affine2D) = Affine2D(
        a * other.a + c * other.b, b * other.a + d * other.b,
        a * other.c + c * other.d, b * other.c + d * other.d,
        a * other.tx + c * other.ty + tx, b * other.tx + d * other.ty + ty,
    )

    fun map(x: Float, y: Float) = (a * x + c * y + tx) to (b * x + d * y + ty)

    fun map(bounds: EditorBounds): EditorBounds {
        val corners = listOf(
            map(bounds.left, bounds.top), map(bounds.right, bounds.top),
            map(bounds.right, bounds.bottom), map(bounds.left, bounds.bottom),
        )
        return EditorBounds(corners.minOf { it.first }, corners.minOf { it.second },
            corners.maxOf { it.first }, corners.maxOf { it.second })
    }

    companion object {
        fun around(x: Float, y: Float, a: Float, b: Float, c: Float, d: Float) =
            Affine2D(a, b, c, d, x - a * x - c * y, y - b * x - d * y)

        fun rotation(angle: Float, width: Int, height: Int, x: Float, y: Float): Affine2D {
            val aspect = width.toFloat() / height
            return around(x, y, cos(angle), sin(angle) * aspect, -sin(angle) / aspect, cos(angle))
        }
    }
}

data class EditorSelection(val id: Long, val bounds: EditorBounds, val transform: Affine2D)

internal class PenHoldGesture(private val slop: Float, private val delayMillis: Long = 550) {
    var pending = false
        private set
    var origin = 0f to 0f
        private set
    private var deadline = 0L

    fun begin(x: Float, y: Float, nowMillis: Long) {
        origin = x to y
        deadline = nowMillis + delayMillis
        pending = true
    }

    fun move(x: Float, y: Float) {
        if (hypot(x - origin.first, y - origin.second) > slop) pending = false
    }

    fun takeLongPress(nowMillis: Long): Boolean {
        if (!pending || nowMillis < deadline) return false
        pending = false
        return true
    }

    fun cancel() { pending = false }
}
