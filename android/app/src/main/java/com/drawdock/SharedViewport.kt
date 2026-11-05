package com.drawdock

import kotlin.math.hypot

data class SharedViewport(val scale: Float = 1f, val left: Float = 0f, val top: Float = 0f) {
    init {
        require(scale.isFinite() && scale in 1f..4f && left.isFinite() && top.isFinite())
        val edge = 1f - 1f / scale
        require(left >= 0f && top >= 0f && left <= edge + 0.000001f && top <= edge + 0.000001f)
    }

    val matrix get() = Affine2D(a = scale, d = scale, tx = -left * scale, ty = -top * scale)
    fun source(x: Float, y: Float) = left + x / scale to top + y / scale

    fun navigate(factor: Float, fromX: Float, fromY: Float, toX: Float, toY: Float): SharedViewport {
        require(factor.isFinite() && factor > 0f)
        val next = (scale * factor).coerceIn(1f, 4f)
        val anchor = source(fromX, fromY)
        val edge = 1f - 1f / next
        return SharedViewport(next, (anchor.first - toX / next).coerceIn(0f, edge),
            (anchor.second - toY / next).coerceIn(0f, edge))
    }

    fun mark(mark: InkMark, width: Int, height: Int): InkMark = mark.copy(
        width = mark.width / scale,
        points = mark.points.map {
            val point = source(it.first / width, it.second / height)
            point.first * width to point.second * height
        },
    )
}

internal class TwoFingerNavigation(
    val firstId: Int, val secondId: Int, private val initial: SharedViewport,
    first: Pair<Float, Float>, second: Pair<Float, Float>,
    private val width: Int, private val height: Int,
) {
    private val distance = hypot(first.first - second.first, first.second - second.second).coerceAtLeast(1f)
    private val centerX = (first.first + second.first) / (2 * width)
    private val centerY = (first.second + second.second) / (2 * height)

    fun update(first: Pair<Float, Float>, second: Pair<Float, Float>): SharedViewport =
        initial.navigate(
            (hypot(first.first - second.first, first.second - second.second) / distance).coerceAtLeast(0.001f),
            centerX, centerY,
            (first.first + second.first) / (2 * width),
            (first.second + second.second) / (2 * height),
        )
}
