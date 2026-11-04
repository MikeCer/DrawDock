package com.drawdock

data class ViewportFit(val width: Int, val height: Int, val left: Int, val top: Int) {
    companion object {
        fun calculate(viewWidth: Int, viewHeight: Int, sourceWidth: Int, sourceHeight: Int): ViewportFit {
            require(viewWidth > 0 && viewHeight > 0 && sourceWidth > 0 && sourceHeight > 0)
            val scale = minOf(viewWidth.toDouble() / sourceWidth, viewHeight.toDouble() / sourceHeight)
            val width = (sourceWidth * scale).toInt().coerceIn(1, viewWidth)
            val height = (sourceHeight * scale).toInt().coerceIn(1, viewHeight)
            return ViewportFit(width, height, (viewWidth - width) / 2, (viewHeight - height) / 2)
        }
    }
}
