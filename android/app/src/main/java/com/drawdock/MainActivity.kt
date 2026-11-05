package com.drawdock

import android.content.Intent
import android.content.ClipboardManager
import android.graphics.Color
import android.graphics.SurfaceTexture
import android.os.Bundle
import android.view.Gravity
import android.view.ViewGroup
import android.view.View
import android.view.TextureView
import android.view.Surface
import android.view.WindowInsets
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import android.widget.FrameLayout
import android.widget.Spinner
import android.widget.ArrayAdapter
import android.widget.AdapterView
import android.widget.HorizontalScrollView
import android.text.InputType
import android.text.InputFilter
import android.widget.Toast
import androidx.activity.ComponentActivity
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import org.json.JSONArray
import org.json.JSONObject
import kotlin.concurrent.thread

class MainActivity : ComponentActivity() {
    private val qrScanner = registerForActivityResult(ScanContract()) { result ->
        handleQrResult(result.contents)
    }
    private lateinit var inputView: InkInputView
    private lateinit var connectionButton: Button
    private lateinit var statusText: TextView
    private lateinit var screenSelector: Spinner
    private lateinit var workspace: FrameLayout
    private lateinit var videoTexture: TextureView
    private lateinit var toolsMenu: ScrollView
    private lateinit var menuButton: Button
    private lateinit var radialMenu: RadialMenuView
    private var blackboard = false
    private var grid = false
    private var pendingEditorRequestId: Long? = null
    private var clipboardImportPending = false
    private var videoSurface: Surface? = null
    private var pendingConnection: (() -> Unit)? = null
    private var sources = emptyList<Pair<String, String>>()
    private var sourceName = ""
    private var sourceEpoch = 0L
    private var pendingSourceRequestId: Long? = null
    private var pendingReadyEpoch: Long? = null
    private var readyEpoch: Long? = null
    private var sessionActive = false
    private var videoWidth = 0
    private var videoHeight = 0
    private var videoFailed = false
    private var sessionClient: SessionClient? = null
    private val rememberedHostStore by lazy { RememberedHostStore(this) }
    private var sessionGeneration = 0L
    private var pendingModeRequestId: Long? = null
    private var pendingClearRequestId: Long? = null
    private var selectedTool = DrawTool.FREEHAND
    private var selectedColor = Color.RED
    private var selectedWidth = 6f

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        inputView = InkInputView(this)
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(0xFFF4F4F4.toInt())
            setOnApplyWindowInsetsListener { view, insets ->
                val bars = insets.getInsets(WindowInsets.Type.systemBars())
                view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
                insets
            }
        }
        val header = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
        }
        screenSelector = Spinner(this).apply {
            contentDescription = "PC screen to mirror"
            isEnabled = false
        }
        screenSelector.adapter = ArrayAdapter(this, android.R.layout.simple_spinner_dropdown_item,
            listOf("Connect to select a PC screen"))
        header.addView(screenSelector, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        menuButton = button("Hide settings") {
            val show = toolsMenu.visibility != View.VISIBLE
            toolsMenu.visibility = if (show) View.VISIBLE else View.GONE
            text = if (show) "Hide settings" else "Settings"
        }
        header.addView(button("Pen tools") {
            inputView.cancelActiveContact(keepPath = true)
            showRadialMenu(workspace.width / 2f, workspace.height / 2f)
        })
        header.addView(menuButton)
        root.addView(header)
        workspace = FrameLayout(this).apply { setBackgroundColor(Color.BLACK) }
        root.addView(workspace, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f))
        videoTexture = TextureView(this).apply { alpha = 0f }
        workspace.addView(videoTexture, FrameLayout.LayoutParams(-1, -1, Gravity.CENTER))
        workspace.addView(inputView, FrameLayout.LayoutParams(-1, -1, Gravity.CENTER))
        videoTexture.surfaceTextureListener = object : TextureView.SurfaceTextureListener {
            override fun onSurfaceTextureAvailable(texture: SurfaceTexture, width: Int, height: Int) {
                videoSurface = Surface(texture)
                pendingConnection?.let { connect ->
                    pendingConnection = null
                    connect()
                }
            }
            override fun onSurfaceTextureSizeChanged(texture: SurfaceTexture, width: Int, height: Int) = Unit
            override fun onSurfaceTextureUpdated(texture: SurfaceTexture) = Unit
            override fun onSurfaceTextureDestroyed(texture: SurfaceTexture): Boolean {
                disconnectSession()
                videoSurface?.release()
                videoSurface = null
                return true
            }
        }
        workspace.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ -> fitWorkspace() }
        screenSelector.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onNothingSelected(parent: AdapterView<*>?) = Unit
            override fun onItemSelected(parent: AdapterView<*>?, view: View?, position: Int, id: Long) {
                val selected = sources.getOrNull(position)?.first ?: return
                if (!sessionActive || videoFailed || readyEpoch != sourceEpoch ||
                    selected == sourceName || pendingSourceRequestId != null) return
                inputView.cancelActiveContact()
                inputView.setInputReady(false)
                val request = sessionClient?.selectSource(selected)
                if (request == null) {
                    statusText.text = "Screen change was not queued."
                    screenSelector.setSelection(sources.indexOfFirst { it.first == sourceName }.coerceAtLeast(0))
                    refreshInputReady()
                } else {
                    pendingSourceRequestId = request
                    screenSelector.isEnabled = false
                    statusText.text = "Waiting for the selected PC screen…"
                    workspace.postDelayed({
                        if (pendingSourceRequestId == request) {
                            disconnectSession()
                            statusText.text = "Screen change was not acknowledged; reconnect required."
                        }
                    }, 10_000)
                }
            }
        }

        val controls = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(8, 8, 8, 8)
            setBackgroundColor(0xEEF4F4F4.toInt())
            isClickable = true
        }
        val topRow = row(controls)
        connectionButton = button("Connect") {
            if (sessionClient == null) connectRememberedOrPair() else disconnectSession()
        }.also(topRow::addView)

        statusText = TextView(this).apply {
            text = "Not connected; tablet preview alone is not PC annotation."
            setTextColor(Color.DKGRAY)
            textSize = 12f
        }
        val pairingRow = row(controls)
        button("Pair another PC") {
            disconnectSession()
            showConnectionDialog()
        }.also(pairingRow::addView)
        button("Forget PC") { forgetRememberedHost() }.also(pairingRow::addView)
        controls.addView(statusText)
        toolsMenu = ScrollView(this).apply { addView(controls); elevation = 8f }
        workspace.addView(toolsMenu, FrameLayout.LayoutParams(-1, -2, Gravity.TOP))
        radialMenu = RadialMenuView(this)
        workspace.addView(radialMenu, FrameLayout.LayoutParams(-1, -1))
        setContentView(root)
        inputView.onCommit = { mark ->
            val client = sessionClient
            if (client == null) {
                statusText.text = "LOCAL PREVIEW ONLY - no host session"
            } else if (!client.sendMark(mark, inputView.width, inputView.height)) {
                statusText.text = "Stroke was not sent; see connection status"
            }
        }
        inputView.onUndo = { sessionClient?.undo() }
        inputView.onRedo = { sessionClient?.redo() }
        inputView.onMessage = { notifyDrawing(it) }
        inputView.onLongPress = { x, y -> showRadialMenu(x + inputView.left, y + inputView.top) }
        inputView.onSelect = { x, y ->
            sendEditor(JSONObject().put("type", "select_object").put("x", x.toDouble()).put("y", y.toDouble()))
        }
        inputView.onTransform = { id, transform -> sendTransform(id, transform) }
        inputView.onRequestText = { x, y -> requestText(x, y) }
        inputView.onLaser = { active, x, y, color ->
            if (sessionActive && sessionClient?.sendLaser(active, x, y, color, inputView.width, inputView.height) != true) {
                notifyDrawing("Laser update was not queued.")
            }
        }
        inputView.onPointerEvent = { event ->
            val client = sessionClient
            if (client == null || !client.sendPointer(event, inputView.width, inputView.height)) {
                statusText.text = "Pointer event was not sent"
            }
        }
        configureInput()
        connectFromIntent(intent)
    }

    override fun onPause() {
        pendingConnection = null
        inputView.cancelActiveContact()
        disconnectSession()
        super.onPause()
    }

    override fun onDestroy() {
        disconnectSession()
        super.onDestroy()
    }

    private fun configureInput() {
        inputView.configure(selectedTool, selectedColor, selectedWidth)
    }

    private fun notifyDrawing(message: String) {
        statusText.text = message
        Toast.makeText(this, message, Toast.LENGTH_SHORT).show()
    }

    private fun showRadialMenu(x: Float, y: Float) {
        radialMenu.open(x, y)
        radialRoot()
    }

    private fun radialAction(label: String, action: () -> Unit) =
        RadialMenuView.Item(label) { radialMenu.dismiss(); action() }

    private fun radialRoot() {
        radialMenu.page(listOf(
            RadialMenuView.Item("Pen") { penPage() },
            RadialMenuView.Item("Shapes") { shapesPage() },
            RadialMenuView.Item("Select /\nEdit") { editPage() },
            RadialMenuView.Item("Style") { stylePage() },
            RadialMenuView.Item("Board /\nGrid") { canvasPage() },
            radialAction("Undo") { history(false) },
            radialAction("Redo") { history(true) },
            RadialMenuView.Item("More") { morePage() },
        ))
    }

    private fun toolItem(label: String, tool: DrawTool) = radialAction(label) { pickTool(tool) }

    private fun penPage() {
        radialMenu.page(listOf(toolItem("Free", DrawTool.FREEHAND),
            toolItem("High-\nlighter", DrawTool.HIGHLIGHTER), toolItem("Laser", DrawTool.LASER)), ::radialRoot)
    }

    private fun shapesPage() {
        radialMenu.page(listOf(
            toolItem("Rectangle", DrawTool.RECTANGLE), toolItem("Circle", DrawTool.CIRCLE),
            toolItem("Ellipse", DrawTool.ELLIPSE), toolItem("Line", DrawTool.LINE),
            toolItem("Polygon", DrawTool.POLYGON), toolItem("Polyline", DrawTool.POLYLINE),
            toolItem("Arrow", DrawTool.ARROW), toolItem("Text", DrawTool.TEXT),
        ), ::radialRoot)
    }

    private fun editPage() {
        radialMenu.page(listOf(
            toolItem("Select /\nMove", DrawTool.SELECT_MOVE), toolItem("Rotate", DrawTool.ROTATE),
            toolItem("Resize", DrawTool.RESIZE),
            radialAction("Mirror\nH") { mirror(true) }, radialAction("Mirror\nV") { mirror(false) },
            radialAction("Deselect") { inputView.selection = null },
        ), ::radialRoot)
    }

    private fun stylePage() {
        radialMenu.page(listOf(
            RadialMenuView.Item("Color") {
                radialMenu.page(listOf("Red" to Color.RED, "Orange" to 0xFFFF9800.toInt(),
                    "Yellow" to Color.YELLOW, "Green" to Color.GREEN, "Blue" to Color.BLUE,
                    "Black" to Color.BLACK, "White" to Color.WHITE).map { (label, color) ->
                    radialAction(label) { selectedColor = color; configureInput() }
                }, ::stylePage)
            },
            RadialMenuView.Item("Width") {
                radialMenu.page(listOf("Thin" to 3f, "Medium" to 6f, "Thick" to 12f).map { (label, width) ->
                    radialAction(label) {
                        selectedWidth = width * resources.displayMetrics.density
                        configureInput()
                    }
                }, ::stylePage)
            },
        ), ::radialRoot)
    }

    private fun canvasPage() {
        radialMenu.page(listOf(
            radialAction(if (blackboard) "Blackboard\nOFF" else "Blackboard\nON") {
                sendEditor(JSONObject().put("type", "set_canvas").put("blackboard", !blackboard).put("grid", grid))
            },
            radialAction(if (grid) "Grid OFF" else "Grid ON") {
                sendEditor(JSONObject().put("type", "set_canvas").put("blackboard", blackboard).put("grid", !grid))
            },
        ), ::radialRoot)
    }

    private fun morePage() {
        radialMenu.page(listOf(
            radialAction(if (inputView.drawMode) "Pointer\nmode" else "Draw\nmode") { requestModeChange() },
            radialAction("Finish\npath") { inputView.finishPath() },
            radialAction("Cancel\npath") { inputView.cancelActiveContact() },
            radialAction("Paste\nimage") { pasteClipboardImage() },
            radialAction("Clear\nall") { confirmRemoteClear() },
            radialAction("Settings") { toolsMenu.visibility = View.VISIBLE; menuButton.text = "Hide settings" },
        ), ::radialRoot)
    }

    private fun pickTool(tool: DrawTool) {
        if (tool in listOf(DrawTool.LASER, DrawTool.SELECT_MOVE, DrawTool.ROTATE, DrawTool.RESIZE) && !sessionActive) {
            notifyDrawing("Connect to the PC to use this tool.")
            return
        }
        selectedTool = tool
        if (tool !in listOf(DrawTool.SELECT_MOVE, DrawTool.ROTATE, DrawTool.RESIZE)) inputView.selection = null
        configureInput()
        if (!inputView.drawMode) requestModeChange()
        else notifyDrawing("${tool.name.lowercase().replace('_', ' ')} - long-press the pen for tools.")
    }

    private fun history(redo: Boolean) {
        inputView.cancelActiveContact()
        inputView.selection = null
        if (sessionActive) {
            if (!editorReady()) { notifyDrawing("Wait until the PC screen is ready."); return }
            val sent = if (redo) sessionClient?.redo() else sessionClient?.undo()
            if (sent != true) notifyDrawing("History command was not queued.")
        } else if (redo) inputView.redo() else inputView.undo()
    }

    private fun editorReady() =
        sessionActive && readyEpoch == sourceEpoch && !videoFailed && pendingSourceRequestId == null &&
            pendingEditorRequestId == null && pendingClearRequestId == null && pendingModeRequestId == null

    private fun sendEditor(message: JSONObject): Boolean {
        if (!editorReady()) { notifyDrawing("Connect and wait until the PC screen is ready."); return false }
        val client = sessionClient ?: return false
        val request = client.editor(message)
        if (request == null) { notifyDrawing("Drawing command was not queued."); return false }
        pendingEditorRequestId = request
        refreshInputReady()
        workspace.postDelayed({
            if (sessionClient === client && pendingEditorRequestId == request) {
                disconnectSession()
                notifyDrawing("Drawing command was not acknowledged; reconnect required.")
            }
        }, 10_000)
        return true
    }

    private fun transformJson(value: Affine2D) = JSONObject()
        .put("a", value.a.toDouble()).put("b", value.b.toDouble()).put("c", value.c.toDouble())
        .put("d", value.d.toDouble()).put("tx", value.tx.toDouble()).put("ty", value.ty.toDouble())

    private fun sendTransform(id: Long, transform: Affine2D): Boolean =
        sendEditor(JSONObject().put("type", "transform_object").put("object_id", id)
            .put("transform", transformJson(transform)))

    private fun mirror(horizontal: Boolean) {
        val selected = inputView.selection
        if (selected == null) { notifyDrawing("Use Select / Move and tap an object first."); return }
        val delta = Affine2D.around(selected.center.first, selected.center.second,
            if (horizontal) -1f else 1f, 0f, 0f, if (horizontal) 1f else -1f)
        sendTransform(selected.id, delta * selected.transform)
    }

    private fun applyEditorEvent(response: JSONObject) {
        if (response.optLong("source_epoch", -1) != sourceEpoch) return
        val request = response.optLong("request_id", -1)
        if (response.optString("type") == "error") {
            if (pendingEditorRequestId == request) {
                pendingEditorRequestId = null
                inputView.cancelSelectionPreview()
            }
            if (pendingModeRequestId == request) pendingModeRequestId = null
            if (pendingClearRequestId == request) pendingClearRequestId = null
            refreshInputReady()
            return
        }
        if (pendingEditorRequestId == request) pendingEditorRequestId = null
        response.optJSONObject("canvas")?.let {
            blackboard = it.getBoolean("blackboard")
            grid = it.getBoolean("grid")
        }
        if (response.has("mode")) inputView.drawMode = response.getString("mode") == "draw"
        if (response.has("selection")) {
            inputView.selection = response.optJSONObject("selection")?.let {
                val bounds = it.getJSONObject("bounds")
                val local = it.getJSONObject("local_bounds")
                val matrix = it.getJSONObject("transform")
                EditorSelection(it.getLong("id"), EditorBounds(
                    bounds.getDouble("left").toFloat(), bounds.getDouble("top").toFloat(),
                    bounds.getDouble("right").toFloat(), bounds.getDouble("bottom").toFloat()),
                    Affine2D(matrix.getDouble("a").toFloat(), matrix.getDouble("b").toFloat(),
                        matrix.getDouble("c").toFloat(), matrix.getDouble("d").toFloat(),
                        matrix.getDouble("tx").toFloat(), matrix.getDouble("ty").toFloat()),
                    EditorBounds(local.getDouble("left").toFloat(), local.getDouble("top").toFloat(),
                        local.getDouble("right").toFloat(), local.getDouble("bottom").toFloat()))
            }
            statusText.text = if (inputView.selection == null) "No object selected; long-press for tools."
                else "Drag to move; drag a corner to resize or the curved-arrow handle to rotate."
        }
        refreshInputReady()
    }

    private fun requestText(x: Float, y: Float) {
        val generation = sessionGeneration
        val epoch = sourceEpoch
        val field = EditText(this).apply {
            hint = "Text to place on the PC"
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE
            filters = arrayOf(InputFilter.LengthFilter(512))
            maxLines = 8
        }
        val fontSize = when {
            selectedWidth <= 3f * resources.displayMetrics.density -> 0.022f
            selectedWidth >= 12f * resources.displayMetrics.density -> 0.05f
            else -> 0.035f
        }
        android.app.AlertDialog.Builder(this).setTitle("Place text").setView(field)
            .setNegativeButton("Cancel", null).setPositiveButton("Place") { _, _ ->
                val text = field.text.toString()
                if (text.isBlank()) { notifyDrawing("Text cannot be empty."); return@setPositiveButton }
                if (text.lines().size > 8) { notifyDrawing("Use at most 8 lines per text object."); return@setPositiveButton }
                if (generation != sessionGeneration || epoch != sourceEpoch ||
                    (sessionClient != null && !editorReady())) {
                    notifyDrawing("Text was not placed because the PC screen/session changed.")
                    return@setPositiveButton
                }
                inputView.addText(x, y, text, fontSize)
            }.show()
    }

    private fun pasteClipboardImage() {
        if (!editorReady() || !inputView.drawMode) {
            notifyDrawing("Connect to the PC and enable Draw mode before pasting an image.")
            return
        }
        if (clipboardImportPending) { notifyDrawing("An image is already being read."); return }
        val uri = try {
            getSystemService(ClipboardManager::class.java).primaryClip?.let { clip ->
                if (clip.itemCount > 0) clip.getItemAt(0).uri else null
            }
        } catch (error: SecurityException) {
            notifyDrawing("Tablet clipboard access was denied.")
            return
        }
        if (uri == null) { notifyDrawing("Copy an image to the tablet clipboard, not text or a web link."); return }
        val generation = sessionGeneration
        val epoch = sourceEpoch
        clipboardImportPending = true
        statusText.text = "Reading tablet clipboard image..."
        thread(name = "drawdock-clipboard-image") {
            try {
                val image = ClipboardImage.read(contentResolver, uri)
                runOnUiThread {
                    if (generation != sessionGeneration || epoch != sourceEpoch || !editorReady()) {
                        notifyDrawing("Image was not pasted because the PC screen/session changed.")
                    } else {
                        val scale = minOf(inputView.width * 0.6f / image.width, inputView.height * 0.6f / image.height)
                        val w = image.width * scale
                        val h = image.height * scale
                        val left = (inputView.width - w) / 2
                        val top = (inputView.height - h) / 2
                        val mark = InkMark(DrawTool.IMAGE, Color.WHITE, 0.005f * minOf(inputView.width, inputView.height),
                            listOf(left to top, left + w to top + h), imageData = image.base64)
                        if (sessionClient?.sendMark(mark, inputView.width, inputView.height) == true) {
                            inputView.selection = null
                            pickTool(DrawTool.SELECT_MOVE)
                            notifyDrawing("Image queued (${image.width}x${image.height}); tap it once visible, then drag to position it.")
                        } else notifyDrawing("Image was not queued.")
                    }
                }
            } catch (error: java.io.IOException) {
                runOnUiThread { notifyDrawing("Cannot read clipboard image: ${error.message}") }
            } catch (error: SecurityException) {
                runOnUiThread { notifyDrawing("Clipboard image access denied; copy the image again.") }
            } catch (error: IllegalArgumentException) {
                runOnUiThread { notifyDrawing(error.message ?: "Unsupported clipboard image.") }
            } catch (error: IllegalStateException) {
                runOnUiThread { notifyDrawing(error.message ?: "Cannot prepare clipboard image.") }
            } finally {
                runOnUiThread { clipboardImportPending = false }
            }
        }
    }

    private fun fitWorkspace() {
        if (workspace.width <= 0 || workspace.height <= 0) return
        val fit = ViewportFit.calculate(workspace.width, workspace.height,
            videoWidth.takeIf { it > 0 } ?: workspace.width,
            videoHeight.takeIf { it > 0 } ?: workspace.height)
        for (view in listOf(videoTexture, inputView)) {
            val old = view.layoutParams as FrameLayout.LayoutParams
            if (old.width != fit.width || old.height != fit.height) {
                inputView.cancelActiveContact()
                view.layoutParams = FrameLayout.LayoutParams(fit.width, fit.height, Gravity.CENTER)
            }
        }
        val menuHeight = minOf((240 * resources.displayMetrics.density).toInt(), workspace.height / 2)
        if (toolsMenu.layoutParams.height != menuHeight) {
            toolsMenu.layoutParams = FrameLayout.LayoutParams(-1, menuHeight, Gravity.TOP)
        }
    }

    private fun refreshInputReady() {
        inputView.setInputReady(
            sessionClient == null || (sessionActive && readyEpoch == sourceEpoch && !videoFailed &&
                pendingModeRequestId == null && pendingClearRequestId == null &&
                pendingSourceRequestId == null && pendingEditorRequestId == null),
        )
    }

    private fun applySelectedSource(selected: JSONObject) {
        sourceEpoch = selected.getLong("source_epoch")
        sourceName = selected.getJSONObject("source").getString("name")
        readyEpoch = null
        pendingReadyEpoch = null
        pendingEditorRequestId = null
        blackboard = false
        grid = false
        radialMenu.dismiss()
        inputView.clear()
        inputView.setInputReady(false)
        screenSelector.setSelection(sources.indexOfFirst { it.first == sourceName }.coerceAtLeast(0))
        screenSelector.isEnabled = false
        val generation = sessionGeneration
        val epoch = sourceEpoch
        workspace.postDelayed({
            if (generation == sessionGeneration && sessionClient != null &&
                sourceEpoch == epoch && readyEpoch != epoch) {
                failVideo("PC screen did not become ready within 10 seconds; reconnect required.")
            }
        }, 10_000)
    }

    private fun requestModeChange() {
        val client = sessionClient
        if (client == null) {
            statusText.text = "Connect to the host before enabling Pointer mode."
            return
        }
        if (!editorReady()) { notifyDrawing("Wait until the PC screen is ready."); return }
        if (inputView.drawMode && blackboard) {
            notifyDrawing("Turn Blackboard off before switching to Pointer mode.")
            return
        }
        inputView.cancelActiveContact()
        inputView.setInputReady(false)
        val mode = if (inputView.drawMode) "pointer" else "draw"
        val requestId = client.requestMode(mode)
        if (requestId == null) {
            refreshInputReady()
            statusText.text = "Mode request was not queued."
            return
        }
        pendingModeRequestId = requestId
        statusText.text = "Waiting for host to acknowledge $mode mode…"
        workspace.postDelayed({
            if (sessionClient === client && pendingModeRequestId == requestId) {
                disconnectSession()
                notifyDrawing("Mode change was not acknowledged; reconnect required.")
            }
        }, 5_000)
    }

    private fun row(parent: LinearLayout) = LinearLayout(this).apply row@{
        gravity = Gravity.CENTER_VERTICAL
        orientation = LinearLayout.HORIZONTAL
        val scrolling = HorizontalScrollView(this@MainActivity).apply { addView(this@row) }
        parent.addView(
            scrolling,
            LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT,
            ),
        )
    }

    private fun button(label: String, onClick: Button.() -> Unit) =
        Button(this).apply {
            text = label
            isAllCaps = false
            setOnClickListener { onClick() }
        }

    private fun showConnectionDialog() {
        android.app.AlertDialog.Builder(this)
            .setTitle("Pair with DrawDock host")
            .setItems(arrayOf("Scan QR (recommended)", "Manual invitation (advanced)")) { _, choice ->
                if (choice == 0) {
                    qrScanner.launch(
                        ScanOptions()
                            .setDesiredBarcodeFormats(listOf(ScanOptions.QR_CODE))
                            .setPrompt("Scan the QR code in the DrawDock PC pairing window")
                            .setBeepEnabled(false)
                            .setOrientationLocked(false),
                    )
                } else {
                    showManualConnectionDialog()
                }
            }
            .setNegativeButton("Cancel", null)
            .show()
    }

    private fun connectRememberedOrPair() {
        val saved = try {
            rememberedHostStore.load()
        } catch (error: Exception) {
            statusText.text = "Cannot unlock remembered PC: ${error.message}. Use Forget PC to re-pair."
            return
        }
        if (saved == null) showConnectionDialog()
        else connectToHost(saved.host, saved.port, saved.fingerprint, saved.token, saved.deviceId)
    }

    private fun forgetRememberedHost() {
        android.app.AlertDialog.Builder(this)
            .setMessage("Forget the saved PC on this tablet? Reconnecting will require a new QR invitation.")
            .setNegativeButton("Cancel", null)
            .setPositiveButton("Forget") { _, _ ->
                disconnectSession()
                try {
                    rememberedHostStore.forget()
                    statusText.text = "PC forgotten on this tablet. Host revocation is managed on the PC."
                } catch (error: Exception) {
                    statusText.text = "Could not forget PC: ${error.message}"
                }
            }
            .show()
    }

    private fun handleQrResult(contents: String?) {
        if (contents == null) {
            statusText.text = "QR scan cancelled or camera permission was not granted."
            return
        }
        val invitation = try {
            PairingInvitation.parse(contents)
        } catch (error: IllegalArgumentException) {
            statusText.text = error.message ?: "Invalid DrawDock invitation"
            return
        }
        android.app.AlertDialog.Builder(this)
            .setTitle("Connect to this PC?")
            .setMessage(
                "Host: ${invitation.host}:${invitation.port}\n\n" +
                    "Connect only if you scanned the QR displayed by your own PC. " +
                    "The PC will ask you to approve this tablet. " +
                    "USB connections require ADB reverse; Wi-Fi uses the PC's private LAN IP without USB.",
            )
            .setNegativeButton("Cancel", null)
            .setPositiveButton("Connect") { _, _ ->
                connectToHost(
                    invitation.host, invitation.port, invitation.fingerprint, invitation.token,
                )
            }
            .show()
    }

    private fun showManualConnectionDialog() {
        val fields = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(24, 8, 24, 0)
        }
        fun field(hintText: String, inputType: Int, initial: String = "") =
            EditText(this).apply {
                hint = hintText
                setText(initial)
                this.inputType = inputType
            }.also(fields::addView)

        val hostField = field(
            "Host IP (127.0.0.1 for ADB reverse)",
            InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI,
            "127.0.0.1",
        )
        val portField = field(
            "Port",
            InputType.TYPE_CLASS_NUMBER,
            "47831",
        )
        val fingerprintField = field(
            "Host SHA-256 certificate fingerprint",
            InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS,
        )
        val tokenField = field(
            "Single-use pairing token",
            InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS,
        )
        android.app.AlertDialog.Builder(this)
            .setTitle("Pair with DrawDock host")
            .setView(ScrollView(this).apply { addView(fields) })
            .setNegativeButton("Cancel", null)
            .setPositiveButton("Connect") { _, _ ->
                val port = portField.text.toString().toIntOrNull()
                if (port == null || port !in 1..65535 || hostField.text.isBlank()) {
                    statusText.text = "Enter a host and valid TCP port"
                    return@setPositiveButton
                }
                connectToHost(
                    hostField.text.toString().trim(),
                    port,
                    fingerprintField.text.toString().trim(),
                    tokenField.text.toString().trim(),
                )
            }
            .show()
    }

    private fun connectFromIntent(intent: Intent?) {
        val host = intent?.getStringExtra(EXTRA_HOST)
        val port = intent?.getIntExtra(EXTRA_PORT, -1) ?: -1
        val fingerprint = intent?.getStringExtra(EXTRA_FINGERPRINT)
        val token = intent?.getStringExtra(EXTRA_TOKEN)
        if (host == null && fingerprint == null && token == null) return
        if (host.isNullOrBlank() || port !in 1..65535 ||
            fingerprint.isNullOrBlank() || token.isNullOrBlank()
        ) {
            statusText.text = "ADB pairing data is incomplete; no connection was started."
            return
        }
        connectToHost(host, port, fingerprint, token)
    }

    private fun connectToHost(host: String, port: Int, fingerprint: String, token: String, deviceId: String? = null) {
        val surface = videoSurface
        if (surface == null) {
            pendingConnection = { connectToHost(host, port, fingerprint, token, deviceId) }
            statusText.text = "Waiting for the video surface…"
            return
        }
        sessionGeneration++
        val generation = sessionGeneration
        val client = SessionClient(
            host,
            port,
            fingerprint,
            token,
            onStatus = { message, connected ->
                if (generation == sessionGeneration) {
                    statusText.text = message
                    inputView.setConnectionStatus(
                        if (connected) "HOST SESSION ACTIVE - Draw commits go to the PC"
                        else "LOCAL PREVIEW ONLY - no host session",
                    )
                    sessionActive = connected
                    if (connected) {
                        refreshInputReady()
                        window.addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                        connectionButton.text = "Disconnect"
                        screenSelector.isEnabled = readyEpoch == sourceEpoch && pendingSourceRequestId == null
                    } else {
                        inputView.setInputReady(false)
                        inputView.drawMode = true
                        pendingModeRequestId = null
                        pendingClearRequestId = null
                        pendingEditorRequestId = null
                        inputView.selection = null
                        blackboard = false
                        grid = false
                        radialMenu.dismiss()
                        screenSelector.isEnabled = false
                        readyEpoch = null
                        inputView.remoteVideoVisible = false
                        videoTexture.alpha = 0f
                        window.clearFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                    }
                    if (!connected && !message.startsWith("Connecting")) {
                        sessionClient = null
                        connectionButton.text = "Connect"
                        toolsMenu.visibility = View.VISIBLE
                        menuButton.text = "Hide settings"
                        refreshInputReady()
                    }
                }
            },
            onModeAck = { mode, requestId ->
                if (generation == sessionGeneration && requestId == pendingModeRequestId) {
                    pendingModeRequestId = null
                    inputView.drawMode = mode == "draw"
                    if (mode == "pointer") inputView.selection = null
                    refreshInputReady()
                    statusText.text = "Host acknowledged ${mode} mode"
                }
            },
            onClearAck = { requestId ->
                if (generation == sessionGeneration && requestId == pendingClearRequestId) {
                    pendingClearRequestId = null
                    inputView.clear()
                    refreshInputReady()
                    blackboard = false
                    grid = false
                    statusText.text = "Tablet and PC cleared, including Blackboard and Grid."
                }
            },
            onEditorEvent = { response ->
                if (generation == sessionGeneration) {
                    try {
                        applyEditorEvent(response)
                    } catch (error: org.json.JSONException) {
                        disconnectSession()
                        notifyDrawing("Invalid PC drawing response; reconnect required.")
                    }
                }
            },
            videoSurface = surface,
            onSources = { list, selected ->
                if (generation == sessionGeneration) {
                    sources = (0 until list.length()).map { index ->
                        val output = list.getJSONObject(index)
                        val name = output.getString("name")
                        val width = output.getInt("right") - output.getInt("left")
                        val height = output.getInt("bottom") - output.getInt("top")
                        name to "$name ($width x $height)"
                    }
                    screenSelector.adapter = ArrayAdapter(this,
                        android.R.layout.simple_spinner_dropdown_item, sources.map { it.second })
                    applySelectedSource(selected)
                    videoFailed = false
                }
            },
            onSourceSelected = { selected, requestId ->
                if (generation == sessionGeneration && requestId == pendingSourceRequestId) {
                    pendingSourceRequestId = null
                    applySelectedSource(selected)
                    statusText.text = "Loading $sourceName…"
                }
            },
            onSourceReady = { epoch ->
                if (generation == sessionGeneration && epoch == sourceEpoch) {
                    readyEpoch = epoch
                    pendingReadyEpoch = null
                    inputView.remoteVideoVisible = true
                    screenSelector.isEnabled = true
                    refreshInputReady()
                    toolsMenu.visibility = View.GONE
                    menuButton.text = "Settings"
                    statusText.text = "Live $sourceName - ${videoWidth}x$videoHeight H.264 / included host ink"
                }
            },
            onVideoGeometry = { epoch, width, height ->
                if (generation == sessionGeneration && epoch >= sourceEpoch) {
                    videoWidth = width
                    videoHeight = height
                    fitWorkspace()
                }
            },
            onVideoRendered = { epoch ->
                if (generation == sessionGeneration && epoch == sourceEpoch) {
                    videoTexture.alpha = 1f
                    if (pendingSourceRequestId == null && readyEpoch != epoch && pendingReadyEpoch != epoch) {
                        if (sessionClient?.sourceReady(epoch) == true) {
                            pendingReadyEpoch = epoch
                        } else {
                            statusText.text = "Video geometry acknowledgement was not queued."
                        }
                    }
                }
            },
            onVideoError = { message ->
                if (generation == sessionGeneration) {
                    failVideo("Video unavailable: $message. Reconnect required.")
                }
            },
            rememberedDeviceId = deviceId,
            onRemembered = { id, credential ->
                val pin = fingerprint.replace(":", "").replace(" ", "").lowercase()
                rememberedHostStore.save(RememberedHost(host, port, pin, id, credential))
            },
        )
        sessionClient = client
        inputView.setInputReady(false)
        connectionButton.text = "Connecting…"
        client.connect()
    }

    private fun confirmRemoteClear() {
        android.app.AlertDialog.Builder(this)
            .setMessage("Clear all drawings, text and images on the tablet and PC, and turn Blackboard and Grid off?")
            .setNegativeButton("Cancel", null)
            .setPositiveButton("Clear") { _, _ ->
                val client = sessionClient
                if (client == null) {
                    inputView.clear()
                    statusText.text = "Tablet cleared; no PC session is connected."
                    return@setPositiveButton
                }
                if (!editorReady()) {
                    notifyDrawing("Wait until the PC screen is ready before clearing.")
                    return@setPositiveButton
                }
                inputView.setInputReady(false)
                val requestId = client.clearDisplay()
                if (requestId == null) {
                    refreshInputReady()
                    statusText.text = "Clear was not queued; annotations were not cleared."
                } else {
                    pendingClearRequestId = requestId
                    statusText.text = "Waiting for the PC to acknowledge Clear…"
                    inputView.postDelayed({
                        if (pendingClearRequestId == requestId && sessionClient === client) {
                            pendingClearRequestId = null
                            refreshInputReady()
                            statusText.text = "Clear was not acknowledged; PC state is unknown."
                        }
                    }, 5_000)
                }
            }
            .show()
    }

    private fun failVideo(message: String) {
        disconnectSession()
        toolsMenu.visibility = View.VISIBLE
        menuButton.text = "Hide settings"
        notifyDrawing(message)
    }

    private fun disconnectSession() {
        sessionGeneration++
        inputViewOrNullCancel()
        sessionClient?.disconnect()
        sessionClient = null
        sessionActive = false
        readyEpoch = null
        pendingReadyEpoch = null
        pendingSourceRequestId = null
        pendingEditorRequestId = null
        blackboard = false
        grid = false
        if (::radialMenu.isInitialized) radialMenu.dismiss()
        inputView.selection = null
        videoWidth = 0
        videoHeight = 0
        if (::videoTexture.isInitialized) videoTexture.alpha = 0f
        if (::screenSelector.isInitialized) screenSelector.isEnabled = false
        inputView.remoteVideoVisible = false
        pendingModeRequestId = null
        pendingClearRequestId = null
        connectionButtonSafeText("Connect")
        if (::statusText.isInitialized) {
            statusText.text = "Disconnected; tablet preview alone is not PC annotation."
            inputView.setConnectionStatus("LOCAL PREVIEW ONLY - no host session")
        }
        window.clearFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
    }

    private fun connectionButtonSafeText(text: String) {
        if (::connectionButton.isInitialized) connectionButton.text = text
    }

    private fun inputViewOrNullCancel() {
        if (::inputView.isInitialized) {
            inputView.cancelActiveContact()
            inputView.setInputReady(true)
            inputView.drawMode = true
        }
    }

    companion object {
        const val EXTRA_HOST = "com.drawdock.extra.HOST"
        const val EXTRA_PORT = "com.drawdock.extra.PORT"
        const val EXTRA_FINGERPRINT = "com.drawdock.extra.FINGERPRINT"
        const val EXTRA_TOKEN = "com.drawdock.extra.TOKEN"
    }
}
