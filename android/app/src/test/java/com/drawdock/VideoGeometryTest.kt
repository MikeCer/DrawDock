package com.drawdock

import org.junit.Assert.*
import org.junit.Test

class VideoGeometryTest {
    @Test fun acceptsNative1440pAndPortraitWithoutChangingAspect() {
        VideoGeometry(2560, 1440, 30)
        VideoGeometry(810, 1440, 30)
        val fit = ViewportFit.calculate(2560, 1412, 2560, 1440)
        assertTrue(fit.width <= 2560 && fit.height <= 1412)
    }

    @Test fun rejectsOversizedOddOrInvalidRateConfigurations() {
        for (invalid in listOf(
            Triple(3840, 2160, 30), Triple(2561, 1440, 30),
            Triple(1280, 721, 30), Triple(0, 0, 30),
            Triple(2560, 1440, 0), Triple(2560, 1440, 61),
        )) {
            assertThrows(IllegalArgumentException::class.java) {
                VideoGeometry(invalid.first, invalid.second, invalid.third)
            }
        }
    }
}
