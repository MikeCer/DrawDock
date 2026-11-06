package com.drawdock

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class DrawingRecoveryTest {
    private fun commit(id: Long) = JSONObject("""{
        "type":"commit","object":{"id":$id,"tool":"freehand",
        "style":{"rgba":[255,0,0,255],"width":0.01},
        "points":[{"x":0.2,"y":0.4}]}
    }""")

    @Test fun drawingSnapshotRetainsObjectsTransformsAndCanvasAcrossReload() {
        val drawing = DrawingDocument()
        drawing.apply(commit(1))
        drawing.apply(JSONObject("""{"type":"transform_object","object_id":1,
            "transform":{"a":1,"b":0,"c":0,"d":1,"tx":0.2,"ty":0.1}}"""))
        drawing.apply(JSONObject("""{"type":"set_canvas","blackboard":true,"grid":true}"""))
        val recovered = DrawingDocument().apply { read(JSONObject(drawing.json().toString())) }
        assertEquals(1, recovered.objects.length())
        assertEquals(0.2, recovered.objects.getJSONObject(0).getJSONObject("transform").getDouble("tx"), 0.0001)
        assertEquals(true, recovered.canvas.getBoolean("grid"))
        assertEquals(drawing.json().toString(), recovered.json().toString())
    }

    @Test fun undoRedoAndExplicitClearChangeTheRecoverableDrawingNotTransientInput() {
        val drawing = DrawingDocument()
        drawing.apply(commit(1))
        drawing.apply(commit(2))
        drawing.apply(JSONObject("""{"type":"undo"}"""))
        assertEquals(1, drawing.objects.length())
        drawing.apply(JSONObject("""{"type":"redo"}"""))
        assertEquals(2, drawing.objects.length())
        drawing.apply(JSONObject("""{"type":"clear_display"}"""))
        assertEquals(0, drawing.objects.length())
        assertFalse(drawing.canvas.getBoolean("blackboard"))
        drawing.apply(JSONObject("""{"type":"undo"}"""))
        assertEquals(2, drawing.objects.length())
        drawing.resetHistory()
        drawing.apply(JSONObject("""{"type":"undo"}"""))
        assertEquals(2, drawing.objects.length())
    }
}
