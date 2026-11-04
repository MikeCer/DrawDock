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

    fun inverse(): Affine2D {
        val determinant = a * d - b * c
        require(determinant.isFinite() && kotlin.math.abs(determinant) >= 0.0001f)
        return Affine2D(d / determinant, -b / determinant, -c / determinant, a / determinant,
            (c * ty - d * tx) / determinant, (b * tx - a * ty) / determinant)
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

data class EditorSelection(
    val id: Long,
    val bounds: EditorBounds,
    val transform: Affine2D,
    val localBounds: EditorBounds,
) {
    val center get() = transform.map(localBounds.centerX, localBounds.centerY)
    private val localCorners get() = listOf(
        localBounds.left to localBounds.top, localBounds.right to localBounds.top,
        localBounds.right to localBounds.bottom, localBounds.left to localBounds.bottom,
    )

    fun outline(matrix: Affine2D = transform) = localCorners.map { matrix.map(it.first, it.second) }

    internal fun frame(
        width: Int, height: Int, rotationOffset: Float, inset: Float,
        matrix: Affine2D = transform,
    ): SelectionFrame {
        val corners = outline(matrix).map { it.first * width to it.second * height }
        val top = (corners[0].first + corners[1].first) / 2 to
            (corners[0].second + corners[1].second) / 2
        val center = corners.sumOf { it.first.toDouble() }.toFloat() / 4 to
            corners.sumOf { it.second.toDouble() }.toFloat() / 4
        val dx = corners[1].first - corners[0].first
        val dy = corners[1].second - corners[0].second
        val length = hypot(dx, dy)
        var nx = if (length > 0.001f) dy / length else 0f
        var ny = if (length > 0.001f) -dx / length else -1f
        if (nx * (top.first - center.first) + ny * (top.second - center.second) < 0f) {
            nx = -nx
            ny = -ny
        }
        val margin = inset.coerceIn(0f, minOf(width, height) / 2f)
        return SelectionFrame(corners,
            (top.first + nx * rotationOffset).coerceIn(margin, width - margin) to
                (top.second + ny * rotationOffset).coerceIn(margin, height - margin))
    }

    fun resize(corner: Int, start: Pair<Float, Float>, end: Pair<Float, Float>): Affine2D {
        val anchor = localCorners[(corner + 2) % 4]
        val handle = localCorners[corner]
        val inverse = transform.inverse()
        val from = inverse.map(start.first, start.second)
        val to = inverse.map(end.first, end.second)
        fun scale(delta: Float, span: Float): Float {
            val denominator = if (kotlin.math.abs(span) > 0.000001f) span else if (span < 0) -0.01f else 0.01f
            return (1 + delta / denominator).coerceIn(0.05f, 8f)
        }
        return transform * Affine2D.around(anchor.first, anchor.second,
            scale(to.first - from.first, handle.first - anchor.first), 0f, 0f,
            scale(to.second - from.second, handle.second - anchor.second))
    }
}

internal data class SelectionFrame(
    val corners: List<Pair<Float, Float>>,
    val rotationHandle: Pair<Float, Float>,
) {
    // -1 is the rotation handle; corners run clockwise from the original top-left.
    fun hitHandle(x: Float, y: Float, radius: Float, keepMoveCenter: Boolean = false): Int? {
        val cx = corners.sumOf { it.first.toDouble() }.toFloat() / 4
        val cy = corners.sumOf { it.second.toDouble() }.toFloat() / 4
        if (keepMoveCenter && hypot(x - cx, y - cy) <= radius / 2) return null
        return (listOf(rotationHandle) + corners).mapIndexed { index, point ->
            index - 1 to hypot(point.first - x, point.second - y)
        }.minByOrNull { it.second }?.takeIf { it.second <= radius }?.first
    }

    fun contains(x: Float, y: Float, padding: Float): Boolean {
        var inside = false
        for (i in corners.indices) {
            val a = corners[i]
            val b = corners[(i + 1) % corners.size]
            val dx = b.first - a.first
            val dy = b.second - a.second
            val u = (((x - a.first) * dx + (y - a.second) * dy) /
                (dx * dx + dy * dy).coerceAtLeast(0.000001f)).coerceIn(0f, 1f)
            if (hypot(x - a.first - u * dx, y - a.second - u * dy) <= padding) return true
            if ((a.second > y) != (b.second > y) &&
                x < (b.first - a.first) * (y - a.second) / (b.second - a.second) + a.first) {
                inside = !inside
            }
        }
        return inside
    }
}

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
