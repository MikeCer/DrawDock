package com.drawdock

import org.junit.Assert.assertEquals
import org.junit.Test

class ViewportFitTest {
    @Test
    fun letterboxingKeepsImageAndInkCenteredWithoutDistortion() {
        assertEquals(ViewportFit(1600, 900, 0, 50), ViewportFit.calculate(1600, 1000, 2560, 1440))
        assertEquals(ViewportFit(1000, 562, 0, 519), ViewportFit.calculate(1000, 1600, 2560, 1440))
    }

    @Test
    fun portraitSourceFitsWithPillarboxing() {
        assertEquals(ViewportFit(450, 800, 575, 0), ViewportFit.calculate(1600, 800, 1080, 1920))
    }
}
