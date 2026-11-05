package com.drawdock

import kotlin.math.cos
import kotlin.math.sin

internal data class RadialMenuGeometry(
    val centerX: Float,
    val centerY: Float,
    val radius: Float,
    val outerRadius: Float,
    val slot: Int,
) {
    fun itemCenter(index: Int, count: Int): Pair<Float, Float> {
        require(count in 1..8 && index in 0 until count)
        val angle = -Math.PI / 2 + index * 2 * Math.PI / count
        return centerX + (cos(angle) * radius).toFloat() to
            centerY + (sin(angle) * radius).toFloat()
    }

    companion object {
        fun fit(
            width: Int, height: Int, anchorX: Float, anchorY: Float,
            slot: Int, preferredDiameter: Float, edge: Float,
        ): RadialMenuGeometry? {
            require(slot > 0 && preferredDiameter > 0 && edge >= 0)
            // Square accessible buttons need more space than circular icon glyphs.
            val radius = maxOf(preferredDiameter / 2 - slot / 2f - edge, slot * 1.5f + edge)
            val outer = radius + slot / 2f + edge
            if (outer * 2 + edge * 2 > minOf(width, height)) return null
            return RadialMenuGeometry(
                anchorX.coerceIn(outer + edge, width - outer - edge),
                anchorY.coerceIn(outer + edge, height - outer - edge),
                radius, outer, slot,
            )
        }
    }
}
