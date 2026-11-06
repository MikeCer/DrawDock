package com.drawdock

import android.util.AtomicFile
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

internal class DrawingDocument(
    var objects: JSONArray = JSONArray(),
    var canvas: JSONObject = JSONObject().put("blackboard", false).put("grid", false),
) {
    var objectBytes = 128
    private val undo = ArrayDeque<String>()
    private val redo = ArrayDeque<String>()

    fun json() = JSONObject().put("objects", objects).put("canvas", canvas)

    fun apply(command: JSONObject) {
        when (command.getString("type")) {
            "undo" -> {
                if (undo.isEmpty()) return
                pushHistory(redo, json().toString())
                read(JSONObject(undo.removeLast()))
            }
            "redo" -> {
                if (redo.isEmpty()) return
                pushHistory(undo, json().toString())
                read(JSONObject(redo.removeLast()))
            }
            else -> {
                val previous = json().toString()
                when (command.getString("type")) {
                    "commit" -> objects.put(JSONObject(command.getJSONObject("object").toString()))
                    "transform_object" -> {
                        val id = command.getLong("object_id")
                        val target = (0 until objects.length()).map { objects.getJSONObject(it) }
                            .first { it.getLong("id") == id }
                        target.put("transform", command.getJSONObject("transform"))
                    }
                    "set_canvas" -> canvas = JSONObject()
                        .put("blackboard", command.getBoolean("blackboard"))
                        .put("grid", command.getBoolean("grid"))
                    "clear_display" -> {
                        objects = JSONArray()
                        canvas = JSONObject().put("blackboard", false).put("grid", false)
                    }
                    else -> error("Unsupported recovery command")
                }
                if (json().toString() != previous) {
                    pushHistory(undo, previous)
                    redo.clear()
                }
            }
        }
    }

    fun cost(value: JSONObject): Long {
        val items = value.getJSONArray("objects")
        return 2L + (0 until items.length()).sumOf {
            val item = items.getJSONObject(it)
            objectBytes.toLong() + item.getJSONArray("points").length() * 8L +
                (if (item.isNull("text")) 0 else item.getString("text").toByteArray(Charsets.UTF_8).size) +
                (if (item.isNull("image_data")) 0 else item.getString("image_data").length)
        }
    }

    private fun pushHistory(history: ArrayDeque<String>, value: String) {
        history.addLast(value)
        while (history.size > 32 || history.sumOf { cost(JSONObject(it)) } > 8 * 1024 * 1024) {
            history.removeFirst()
        }
    }

    fun read(value: JSONObject) {
        objects = JSONArray(value.getJSONArray("objects").toString())
        canvas = JSONObject(value.getJSONObject("canvas").toString())
    }

    fun resetHistory() {
        undo.clear()
        redo.clear()
    }
}

internal class DrawingRecovery(private val directory: File) {
    private var pin = ""
    private var data = JSONObject()
    var source: String = ""
        private set
    var width = 0
        private set
    var height = 0
        private set
    var document = DrawingDocument()
        private set
    var objectBytes = 128
        set(value) {
            require(value in 1..512) { "Invalid host recovery accounting" }
            field = value
            document.objectBytes = value
        }

    val hasDrawing get() = document.objects.length() > 0 ||
        document.canvas.getBoolean("blackboard") || document.canvas.getBoolean("grid")

    private fun file() = AtomicFile(File(directory, "drawing-$pin.json"))

    fun open(fingerprint: String) {
        require(fingerprint.matches(Regex("[a-f0-9]{64}"))) { "Invalid recovery host identity" }
        if (fingerprint == pin) return
        pin = fingerprint
        data = if (file().baseFile.exists() || File(file().baseFile.path + ".bak").exists()) {
            val bytes = file().readFully()
            require(bytes.size <= 32 * 1024 * 1024) { "Saved drawing exceeds the recovery limit" }
            JSONObject(String(bytes, Charsets.UTF_8))
        } else JSONObject().put("version", 1).put("sources", JSONObject())
        require(data.getInt("version") == 1) { "Unsupported saved drawing format" }
        source = data.optString("selected")
        loadSource()
    }

    fun select(name: String, sourceWidth: Int, sourceHeight: Int) {
        check(pin.isNotEmpty()) { "Recovery host identity is not set" }
        source = name
        loadSource()
        width = sourceWidth
        height = sourceHeight
        save()
    }

    private fun loadSource() {
        val saved = data.getJSONObject("sources").optJSONObject(source)
        document = DrawingDocument()
        document.objectBytes = objectBytes
        if (saved != null) document.read(saved)
        width = saved?.optInt("width") ?: 0
        height = saved?.optInt("height") ?: 0
    }

    fun record(command: JSONObject) {
        val before = document.json().toString()
        try {
            document.apply(command)
            require(document.objects.length() <= 2_000) { "Drawing object limit reached" }
            require(document.cost(document.json()) <= 8 * 1024 * 1024) { "Drawing scene budget reached" }
            require((0 until document.objects.length()).sumOf {
                document.objects.getJSONObject(it).getJSONArray("points").length()
            } <= 200_000) { "Drawing point limit reached" }
            require(document.json().toString().toByteArray(Charsets.UTF_8).size <= 16 * 1024 * 1024) {
                "Recovery drawing is full; no new command was sent"
            }
            save()
        } catch (error: Exception) {
            document.read(JSONObject(before))
            document.resetHistory()
            throw error
        }
    }

    fun rejectObject(id: Long) {
        document.objects = JSONArray().apply {
            for (index in 0 until document.objects.length()) {
                val value = document.objects.getJSONObject(index)
                if (value.getLong("id") != id) put(value)
            }
        }
        document.resetHistory()
        save()
    }

    fun rollback(value: JSONObject) {
        document.read(value)
        document.resetHistory()
        save()
    }

    private fun save() {
        val updated = JSONObject(data.toString()).put("selected", source)
        updated.getJSONObject("sources").put(source, document.json().put("width", width).put("height", height))
        val bytes = updated.toString().toByteArray(Charsets.UTF_8)
        require(bytes.size <= 32 * 1024 * 1024) { "Saved drawings are full; no new command was sent" }
        val storage = file()
        val stream = storage.startWrite()
        try {
            stream.write(bytes)
            storage.finishWrite(stream)
            data = updated
        } catch (error: Exception) {
            storage.failWrite(stream)
            throw error
        }
    }
}
