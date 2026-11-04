package com.drawdock

internal data class VideoGeometry(val width: Int, val height: Int, val fps: Int) {
    init {
        require(width in 2..2560 && height in 2..1440 && width % 2 == 0 && height % 2 == 0) {
            "Invalid video dimensions (maximum 2560 x 1440, even dimensions required)"
        }
        require(fps in 1..60) { "Invalid video frame rate" }
    }
}
