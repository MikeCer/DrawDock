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
import android.util.TypedValue
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
    private var refreshRadialPage: (() -> Unit)? = null
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
            setBackgroundColor(getColor(R.color.dd_ui_background))
            setOnApplyWindowInsetsListener { view, insets ->
                val bars = insets.getInsets(WindowInsets.Type.systemBars())
                view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
                insets
            }
        }
        val header = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            val padding = uiSize(R.dimen.dd_ui_space)
            setPadding(padding, padding, padding, padding)
            setBackgroundColor(getColor(R.color.dd_ui_background))
        }
        screenSelector = Spinner(this, Spinner.MODE_DROPDOWN).apply {
            contentDescription = "PC screen to mirror"
            isEnabled = false
            background = getDrawable(R.drawable.dd_field_background)
            setPopupBackgroundDrawable(getDrawable(R.drawable.dd_panel_background))
            dropDownVerticalOffset = uiSize(R.dimen.dd_ui_space)
        }
        screenSelector.adapter = screenAdapter(listOf("Connect to select a PC screen"))
        header.addView(screenSelector, LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f))
        menuButton = button("Hide settings", R.drawable.ic_dd_settings) {
            val show = toolsMenu.visibility != View.VISIBLE
            toolsMenu.visibility = if (show) View.VISIBLE else View.GONE
            text = if (show) "Hide settings" else "Settings"
        }
        header.addView(button("Pen tools", R.drawable.ic_dd_pen_tools) {
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
            val padding = uiSize(R.dimen.dd_ui_padding)
            setPadding(padding, padding, padding, padding)
            background = getDrawable(R.drawable.dd_panel_background)
            isClickable = true
        }
        controls.addView(uiLabel("Connection", R.dimen.dd_ui_title))
        val topRow = row(controls)
        connectionButton = button("Connect", R.drawable.ic_dd_disconnected, primary = true) {
            if (sessionClient == null) connectRememberedOrPair() else disconnectSession()
        }.also(topRow::addView)

        statusText = uiLabel("Not connected; tablet preview alone is not PC annotation.")
        val pairingRow = row(controls)
        button("Pair another PC") {
            disconnectSession()
            showConnectionDialog()
        }.also(pairingRow::addView)
        button("Forget PC") { forgetRememberedHost() }.also(pairingRow::addView)
        controls.addView(statusText)
        toolsMenu = ScrollView(this).apply {
            addView(controls)
            elevation = resources.getDimension(R.dimen.dd_ui_elevation)
        }
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

    private fun radialAction(
        label: String, icon: Int, description: String = label.replace('\n', ' '), action: () -> Unit,
    ) = RadialMenuView.Item(label, icon, description) { radialMenu.dismiss(); action() }

    private fun radialRoot() {
        refreshRadialPage = ::radialRoot
        radialMenu.page(listOf(
            RadialMenuView.Item("Pen", R.drawable.ic_dd_pen,
                activeFamily = selectedTool in listOf(DrawTool.FREEHAND, DrawTool.HIGHLIGHTER, DrawTool.LASER)) { penPage() },
            RadialMenuView.Item("Shapes", R.drawable.ic_dd_shapes,
                activeFamily = selectedTool in listOf(DrawTool.RECTANGLE, DrawTool.CIRCLE, DrawTool.ELLIPSE,
                    DrawTool.LINE, DrawTool.POLYGON, DrawTool.POLYLINE, DrawTool.ARROW, DrawTool.TEXT)) { shapesPage() },
            RadialMenuView.Item("Select /\nEdit", R.drawable.ic_dd_select_edit,
                activeFamily = selectedTool in listOf(DrawTool.SELECT_MOVE, DrawTool.ROTATE, DrawTool.RESIZE)) { editPage() },
            RadialMenuView.Item("Style", R.drawable.ic_dd_style, previewColor = selectedColor) { stylePage() },
            RadialMenuView.Item("Board /\nGrid", R.drawable.ic_dd_board_grid) { canvasPage() },
            radialAction("Undo", R.drawable.ic_dd_undo) { history(false) },
            radialAction("Redo", R.drawable.ic_dd_redo) { history(true) },
            RadialMenuView.Item("More", R.drawable.ic_dd_more) { morePage() },
        ))
    }

    private fun toolItem(label: String, tool: DrawTool, icon: Int, description: String = label.replace('\n', ' ')) =
        radialAction(label, icon, description) { pickTool(tool) }.copy(selected = selectedTool == tool)

    private fun penPage() {
        refreshRadialPage = ::penPage
        radialMenu.page(listOf(
            toolItem("Free", DrawTool.FREEHAND, R.drawable.ic_dd_freehand, "Freehand pen"),
            toolItem("High-\nlighter", DrawTool.HIGHLIGHTER, R.drawable.ic_dd_highlighter, "Highlighter"),
            toolItem("Laser", DrawTool.LASER, R.drawable.ic_dd_laser_pointer, "Laser pointer"),
        ), ::radialRoot)
    }

    private fun shapesPage() {
        refreshRadialPage = ::shapesPage
        radialMenu.page(listOf(
            toolItem("Rectangle", DrawTool.RECTANGLE, R.drawable.ic_dd_rectangle),
            toolItem("Circle", DrawTool.CIRCLE, R.drawable.ic_dd_circle),
            toolItem("Ellipse", DrawTool.ELLIPSE, R.drawable.ic_dd_ellipse),
            toolItem("Line", DrawTool.LINE, R.drawable.ic_dd_line, "Straight line"),
            toolItem("Polygon", DrawTool.POLYGON, R.drawable.ic_dd_polygon, "Polygon (closed path)"),
            toolItem("Polyline", DrawTool.POLYLINE, R.drawable.ic_dd_polyline, "Polyline (open path)"),
            toolItem("Arrow", DrawTool.ARROW, R.drawable.ic_dd_arrow),
            toolItem("Text", DrawTool.TEXT, R.drawable.ic_dd_text),
        ), ::radialRoot)
    }

    private fun editPage() {
        refreshRadialPage = ::editPage
        radialMenu.page(listOf(
            toolItem("Select /\nMove", DrawTool.SELECT_MOVE, R.drawable.ic_dd_select_move, "Select and move"),
            toolItem("Rotate", DrawTool.ROTATE, R.drawable.ic_dd_rotate, "Rotate selection"),
            toolItem("Resize", DrawTool.RESIZE, R.drawable.ic_dd_resize, "Resize selection"),
            radialAction("Mirror\nH", R.drawable.ic_dd_mirror_horizontal, "Mirror horizontally") { mirror(true) },
            radialAction("Mirror\nV", R.drawable.ic_dd_mirror_vertical, "Mirror vertically") { mirror(false) },
            radialAction("Deselect", R.drawable.ic_dd_deselect) { inputView.selection = null },
        ), ::radialRoot)
    }

    private fun stylePage() {
        refreshRadialPage = ::stylePage
        radialMenu.page(listOf(
            RadialMenuView.Item("Color", R.drawable.ic_dd_color, previewColor = selectedColor) { colorPage() },
            RadialMenuView.Item("Width", R.drawable.ic_dd_width) { widthPage() },
        ), ::radialRoot)
    }

    private fun colorPage() {
        refreshRadialPage = ::colorPage
        radialMenu.page(listOf("Red" to Color.RED, "Orange" to 0xFFFF9800.toInt(),
            "Yellow" to Color.YELLOW, "Green" to Color.GREEN, "Blue" to Color.BLUE,
            "Black" to Color.BLACK, "White" to Color.WHITE).map { (label, color) ->
            radialAction(label, R.drawable.ic_dd_color, "$label ink") {
                selectedColor = color
                configureInput()
            }.copy(swatch = color, selected = selectedColor == color)
        }, ::stylePage)
    }

    private fun widthPage() {
        refreshRadialPage = ::widthPage
        radialMenu.page(listOf(
            Triple("Thin", R.dimen.dd_menu_width_thin, "3 dp"),
            Triple("Medium", R.dimen.dd_menu_width_medium, "6 dp"),
            Triple("Thick", R.dimen.dd_menu_width_thick, "12 dp"),
        ).map { (label, resource, description) ->
            val width = resources.getDimension(resource)
            radialAction(label, R.drawable.ic_dd_width, "$label, $description") {
                selectedWidth = width
                configureInput()
            }.copy(strokeWidth = width, previewColor = selectedColor, selected = selectedWidth == width)
        }, ::stylePage)
    }

    private fun canvasPage() {
        refreshRadialPage = ::canvasPage
        radialMenu.page(listOf(
            radialAction(if (blackboard) "Blackboard\nOFF" else "Blackboard\nON", R.drawable.ic_dd_blackboard,
                if (blackboard) "Turn Blackboard off" else "Turn Blackboard on") {
                sendEditor(JSONObject().put("type", "set_canvas").put("blackboard", !blackboard).put("grid", grid))
            }.copy(toggleOn = blackboard),
            radialAction(if (grid) "Grid OFF" else "Grid ON", R.drawable.ic_dd_grid,
                if (grid) "Turn Grid off" else "Turn Grid on") {
                sendEditor(JSONObject().put("type", "set_canvas").put("blackboard", blackboard).put("grid", !grid))
            }.copy(toggleOn = grid),
        ), ::radialRoot)
    }

    private fun morePage() {
        refreshRadialPage = ::morePage
        radialMenu.page(listOf(
            radialAction(if (inputView.drawMode) "Pointer\nmode" else "Draw\nmode",
                if (inputView.drawMode) R.drawable.ic_dd_mode_pointer else R.drawable.ic_dd_mode_draw,
                if (inputView.drawMode) "Switch to pointer mode" else "Switch to draw mode") { requestModeChange() },
            radialAction("Finish\npath", R.drawable.ic_dd_finish_path, "Finish polygon or polyline") { inputView.finishPath() },
            radialAction("Cancel\npath", R.drawable.ic_dd_cancel_path, "Cancel polygon or polyline") { inputView.cancelActiveContact() },
            radialAction("Paste\nimage", R.drawable.ic_dd_paste_image, "Paste image from tablet clipboard") { pasteClipboardImage() },
            radialAction("Clear\nall", R.drawable.ic_dd_clear_all, "Clear all annotations on tablet and PC") {
                confirmRemoteClear()
            }.copy(danger = true),
            radialAction("Settings", R.drawable.ic_dd_settings, "Connection settings") {
                toolsMenu.visibility = View.VISIBLE
                menuButton.text = "Hide settings"
            },
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
        if (radialMenu.visibility == View.VISIBLE) refreshRadialPage?.invoke()
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
        val content = ScrollView(this).apply {
            val padding = uiSize(R.dimen.dd_ui_padding)
            setPadding(padding, padding, padding, padding)
            addView(field)
        }
        android.app.AlertDialog.Builder(this).setTitle("Place text").setView(content)
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
            }.showBranded()
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
        val menuHeight = minOf(uiSize(R.dimen.dd_ui_panel_height), workspace.height / 2)
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
            ).apply { topMargin = uiSize(R.dimen.dd_ui_space); bottomMargin = uiSize(R.dimen.dd_ui_space) },
        )
    }

    private fun uiSize(resource: Int) = resources.getDimensionPixelSize(resource)

    private fun uiLabel(label: String, size: Int = R.dimen.dd_ui_label) = TextView(this).apply {
        text = label
        setTextSize(TypedValue.COMPLEX_UNIT_PX, resources.getDimension(size))
        setTextColor(getColor(R.color.dd_ui_text))
    }

    private fun screenAdapter(labels: List<String>) =
        ArrayAdapter(this, R.layout.dd_spinner_selected, labels).apply {
            setDropDownViewResource(R.layout.dd_spinner_option)
        }

    private fun buttonIcon(button: Button, icon: Int, primary: Boolean = false) {
        val drawable = requireNotNull(getDrawable(icon)).mutate().apply {
            setTint(getColor(if (primary) R.color.dd_ui_on_accent else R.color.dd_ui_accent))
            val size = uiSize(R.dimen.dd_ui_icon)
            setBounds(0, 0, size, size)
        }
        button.setCompoundDrawablesRelative(drawable, null, null, null)
    }

    private fun button(label: String, icon: Int? = null, primary: Boolean = false, onClick: Button.() -> Unit) =
        Button(this, null, 0, if (primary) R.style.Widget_DrawDock_Button_Primary else R.style.Widget_DrawDock_Button).apply {
            text = label
            isAllCaps = false
            layoutParams = LinearLayout.LayoutParams(-2, -2).apply {
                marginStart = uiSize(R.dimen.dd_ui_space)
            }
            if (icon != null) buttonIcon(this, icon, primary)
            setOnClickListener { onClick() }
        }

    private fun android.app.AlertDialog.Builder.showBranded(danger: Boolean = false): android.app.AlertDialog =
        show().apply {
            getButton(android.app.AlertDialog.BUTTON_POSITIVE)?.let { positive ->
                positive.background = getDrawable(if (danger) R.drawable.dd_button_background
                    else R.drawable.dd_button_primary_background)
                positive.setTextColor(getColor(if (danger) R.color.dd_ui_danger else R.color.dd_ui_on_accent))
                val params = positive.layoutParams
                if (params is ViewGroup.MarginLayoutParams) {
                    params.marginStart = uiSize(R.dimen.dd_ui_space)
                    positive.layoutParams = params
                }
            }
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
            .showBranded()
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
            .showBranded(danger = true)
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
            .showBranded()
    }

    private fun showManualConnectionDialog() {
        val fields = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            val padding = uiSize(R.dimen.dd_ui_padding)
            setPadding(padding, padding, padding, padding)
        }
        fun field(hintText: String, inputType: Int, initial: String = ""): EditText {
            fields.addView(uiLabel(hintText).apply {
                val spacing = uiSize(R.dimen.dd_ui_space)
                setPadding(0, spacing, 0, spacing)
            })
            return EditText(this).apply {
                contentDescription = hintText
                setText(initial)
                this.inputType = inputType
            }.also(fields::addView)
        }

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
            .showBranded()
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
                        connectionButtonSafeText("Disconnect")
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
                        connectionButtonSafeText("Connect")
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
                    screenSelector.adapter = screenAdapter(sources.map { it.second })
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
        connectionButtonSafeText("Connecting…")
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
            .showBranded(danger = true)
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
        if (::connectionButton.isInitialized) {
            connectionButton.text = text
            buttonIcon(connectionButton,
                if (text == "Disconnect") R.drawable.ic_dd_connected else R.drawable.ic_dd_disconnected,
                primary = true)
        }
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
