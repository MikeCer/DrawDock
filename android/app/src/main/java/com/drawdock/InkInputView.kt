package com.drawdock

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.view.MotionEvent
import android.view.PointerIcon
import android.view.View
import android.view.ViewConfiguration
import android.os.SystemClock
import org.json.JSONObject
import kotlin.math.max
import kotlin.math.hypot

data class InkMark(
    val tool: DrawTool,
    val color: Int,
    val width: Float,
    val points: List<Pair<Float, Float>>,
    val text: String? = null,
    val imageData: String? = null,
) {
    fun commitPoints(): List<Pair<Float, Float>> =
        if (tool in listOf(DrawTool.FREEHAND, DrawTool.HIGHLIGHTER, DrawTool.POLYGON, DrawTool.POLYLINE) ||
            points.size < 2) points
        else listOf(points.first(), points.last())
}

data class PointerEvent(
    val action: String,
    val x: Float,
    val y: Float,
    val button: String,
)

class InkInputView(context: Context) : View(context) {
    private val reducer = InputReducer()
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeCap = Paint.Cap.ROUND
        strokeJoin = Paint.Join.ROUND
    }
    private val marks = mutableListOf<InkMark>()
    private val redoMarks = mutableListOf<InkMark>()
    private val pendingInk = PendingInk()
    private var recoveryObjects = emptyList<Pair<InkMark, android.graphics.Matrix>>()
    private val recoveryImages = mutableMapOf<String, Bitmap>()
    private var recoveryCanvas = false to false
    var showRecovery = false
        set(value) { field = value; invalidate() }

    fun setRecovery(document: JSONObject) {
        recoveryImages.values.forEach { it.recycle() }
        recoveryImages.clear()
        val objects = document.getJSONArray("objects")
        val imageData = (0 until objects.length()).mapNotNull { index ->
            objects.getJSONObject(index).let { value ->
                value.optString("image_data").takeIf { !value.isNull("image_data") && it.isNotEmpty() }
            }
        }.distinct()
        val imageBudget = 16 * 1024 * 1024 / maxOf(1, imageData.size)
        for (data in imageData) {
            val bytes = android.util.Base64.decode(data, android.util.Base64.DEFAULT)
            val bounds = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }
            android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
            require(bounds.outWidth in 1..1280 && bounds.outHeight in 1..1280) {
                "Saved image dimensions are invalid"
            }
            var sample = 1
            while (((bounds.outWidth + sample - 1) / sample).toLong() *
                ((bounds.outHeight + sample - 1) / sample) * 4 > imageBudget) sample *= 2
            val options = android.graphics.BitmapFactory.Options().apply { inSampleSize = sample }
            recoveryImages[data] = requireNotNull(
                android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options),
            ) { "Saved image cannot be decoded" }
        }
        recoveryObjects = (0 until objects.length()).map { index ->
            val value = objects.getJSONObject(index)
            val style = value.getJSONObject("style")
            val rgba = style.getJSONArray("rgba")
            val points = value.getJSONArray("points")
            val mark = InkMark(DrawTool.valueOf(value.getString("tool").uppercase()),
                android.graphics.Color.argb(rgba.getInt(3), rgba.getInt(0), rgba.getInt(1), rgba.getInt(2)),
                style.getDouble("width").toFloat(),
                (0 until points.length()).map {
                    val point = points.getJSONObject(it)
                    point.getDouble("x").toFloat() to point.getDouble("y").toFloat()
                }, value.optString("text").takeIf { !value.isNull("text") },
                value.optString("image_data").takeIf { !value.isNull("image_data") })
            val transform = value.optJSONObject("transform")
            val matrix = android.graphics.Matrix()
            if (transform != null) matrix.setValues(floatArrayOf(
                transform.getDouble("a").toFloat(), transform.getDouble("c").toFloat(), transform.getDouble("tx").toFloat(),
                transform.getDouble("b").toFloat(), transform.getDouble("d").toFloat(), transform.getDouble("ty").toFloat(),
                0f, 0f, 1f))
            mark to matrix
        }
        val canvas = document.getJSONObject("canvas")
        recoveryCanvas = canvas.getBoolean("blackboard") to canvas.getBoolean("grid")
        invalidate()
    }
    private var displayedViewport = SharedViewport()
    var sourceEpoch = 0L
    var onHandoffTimeout: (() -> Unit)? = null
    private val handoffTimer = object : Runnable {
        override fun run() {
            val oldest = pendingInk.oldestSubmission ?: return
            if (SystemClock.uptimeMillis() - oldest >= 10_000) {
                onHandoffTimeout?.invoke()
            } else postDelayed(this, 500)
        }
    }
    private val activePoints = mutableListOf<Pair<Float, Float>>()
    private var activeTool = DrawTool.FREEHAND
    private var activeColor = 0xFFFF0000.toInt()
    private var activeWidth = 6f
    private var latchedTool = activeTool
    private var latchedColor = activeColor
    private var latchedWidth = activeWidth
    private var statusMessage = "LOCAL PREVIEW ONLY - not connected to the PC"
    var onCommit: ((InkMark) -> Long?)? = null
    var onUndo: (() -> Unit)? = null
    var onRedo: (() -> Unit)? = null
    var onPointerEvent: ((PointerEvent) -> Unit)? = null
    var remoteVideoVisible = false
        set(value) {
            field = value
            invalidate()
        }
    private var inputReady = true
    private var contactReady = false
    private var pointerButton = "left"
    private var lastPointerPosition = 0f to 0f
    private val hold = PenHoldGesture(ViewConfiguration.get(context).scaledTouchSlop.toFloat())
    private var pointerDownSent = false
    private var holdConsumed = false
    private val vertices = mutableListOf<Pair<Float, Float>>()
    private var vertexStyle: InkMark? = null
    var selection: EditorSelection? = null
        set(value) {
            field = value
            pendingTransform = null
            invalidate()
        }
    private var editStart: EditorSelection? = null
    private var editTransform: Affine2D? = null
    private var pendingTransform: Affine2D? = null
    private var editTool = DrawTool.SELECT_MOVE
    private var resizeCorner = 2
    private val density get() = resources.displayMetrics.density
    private val selectionBlue = 0xFF0099FF.toInt()
    private val rotationCursor: PointerIcon by lazy {
        val size = (32 * density).toInt().coerceAtLeast(32)
        val bitmap = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
        val cursorCanvas = Canvas(bitmap)
        val cursorPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            style = Paint.Style.STROKE
            strokeCap = Paint.Cap.ROUND
            strokeJoin = Paint.Join.ROUND
        }
        cursorPaint.color = 0xFFFFFFFF.toInt()
        cursorPaint.strokeWidth = 5 * density
        drawRotationGlyph(cursorCanvas, cursorPaint, size / 2f, size / 2f, 9 * density)
        cursorPaint.color = 0xFF000000.toInt()
        cursorPaint.strokeWidth = 2 * density
        drawRotationGlyph(cursorCanvas, cursorPaint, size / 2f, size / 2f, 9 * density)
        PointerIcon.create(bitmap, size / 2f, size / 2f)
    }
    private var laserPosition: Pair<Float, Float>? = null
    var onLongPress: ((Float, Float) -> Unit)? = null
    var onSelect: ((Float, Float) -> Unit)? = null
    var onTransform: ((Long, Affine2D) -> Boolean)? = null
    var onRequestText: ((Float, Float) -> Unit)? = null
    var onLaser: ((Boolean, Float, Float, Int) -> Unit)? = null
    var onMessage: ((String) -> Unit)? = null
    var navigationEnabled = false
    var navigationViewport = SharedViewport()
    var onNavigate: ((SharedViewport) -> Unit)? = null
    private var navigation: TwoFingerNavigation? = null
    private var navigationConsumed = false
    private val holdTimer = Runnable {
        if (reducer.currentPointerId != null && hold.takeLongPress(SystemClock.uptimeMillis())) openPenMenu()
    }
    private val laserTimer = object : Runnable {
        override fun run() {
            val point = laserPosition ?: return
            onLaser?.invoke(true, point.first, point.second, activeColor)
            postDelayed(this, 100)
        }
    }
    var drawMode: Boolean = true
        set(value) {
            if (field != value) {
                cancelActiveContact()
                field = value
                invalidate()
            }
        }

    init {
        isFocusable = true
        contentDescription = "Stylus preview canvas; host connection is not active"
    }

    fun configure(tool: DrawTool, color: Int, width: Float) {
        if (activeTool != tool || activeColor != color || activeWidth != width) {
            cancelActiveContact()
        }
        activeTool = tool
        activeColor = color
        activeWidth = width
    }

    fun undo() {
        if (marks.isNotEmpty()) {
            redoMarks.add(marks.removeAt(marks.lastIndex))
            onUndo?.invoke()
        }
        invalidate()
    }

    fun clear() {
        cancelActiveContact()
        redoMarks.clear()
        marks.clear()
        clearPendingInk()
        selection = null
        invalidate()
    }

    fun clearPendingInk() {
        pendingInk.clear()
        displayedViewport = SharedViewport()
        removeCallbacks(handoffTimer)
        invalidate()
    }

    fun acknowledgeInk(id: Long, epoch: Long, revision: Long) {
        pendingInk.acknowledge(id, epoch, revision)
        invalidate()
    }

    fun rejectInk(id: Long) {
        pendingInk.reject(id)
        invalidate()
    }

    fun frameLatched(frame: VideoFrameState) {
        pendingInk.frameLatched(frame)
        frame.viewport?.let { displayedViewport = it }
        invalidate()
    }

    fun cancelActiveContact(keepPath: Boolean = false) {
        if (pointerDownSent) {
            onPointerEvent?.invoke(
                PointerEvent(
                    "cancel",
                    clampX(lastPointerPosition.first),
                    clampY(lastPointerPosition.second),
                    pointerButton,
                ),
            )
        }
        pointerDownSent = false
        hold.cancel()
        removeCallbacks(holdTimer)
        stopLaser()
        reducer.cancel()
        activePoints.clear()
        if (!keepPath) {
            vertices.clear()
            vertexStyle = null
        }
        editStart = null
        editTransform = null
        invalidate()
    }

    fun cancelSelectionPreview() {
        pendingTransform = null
        invalidate()
    }

    fun setInputReady(ready: Boolean) {
        inputReady = ready
        if (!ready) cancelActiveContact()
    }

    fun redo() {
        if (redoMarks.isNotEmpty()) {
            marks.add(redoMarks.removeAt(redoMarks.lastIndex))
            onRedo?.invoke()
        }
        invalidate()
    }

    fun setConnectionStatus(message: String) {
        statusMessage = message
        invalidate()
    }

    private fun handleNavigation(event: MotionEvent): Boolean {
        val action = event.actionMasked
        if (action == MotionEvent.ACTION_DOWN) {
            navigation = null
            navigationConsumed = false
        }
        if (action == MotionEvent.ACTION_CANCEL) {
            val consumed = navigationConsumed
            navigation = null
            navigationConsumed = false
            return consumed
        }
        if (event.pointerCount > 2 || !navigationEnabled) navigation = null
        if (!navigationConsumed && navigationEnabled && inputReady && remoteVideoVisible &&
            action == MotionEvent.ACTION_POINTER_DOWN && event.pointerCount == 2 &&
            reducer.currentPointerId == null &&
            (0..1).all { event.getToolType(it) == MotionEvent.TOOL_TYPE_FINGER }
        ) {
            cancelActiveContact()
            navigationConsumed = true
            navigation = TwoFingerNavigation(event.getPointerId(0), event.getPointerId(1),
                navigationViewport, event.getX(0) to event.getY(0), event.getX(1) to event.getY(1),
                width, height)
        }
        val gesture = navigation
        if (gesture != null && action in listOf(MotionEvent.ACTION_MOVE, MotionEvent.ACTION_POINTER_UP)) {
            val first = event.findPointerIndex(gesture.firstId)
            val second = event.findPointerIndex(gesture.secondId)
            if (first >= 0 && second >= 0 &&
                event.getToolType(first) == MotionEvent.TOOL_TYPE_FINGER &&
                event.getToolType(second) == MotionEvent.TOOL_TYPE_FINGER) {
                onNavigate?.invoke(gesture.update(event.getX(first) to event.getY(first),
                    event.getX(second) to event.getY(second)))
            }
        }
        if (action == MotionEvent.ACTION_POINTER_UP) navigation = null
        val consumed = navigationConsumed
        if (action == MotionEvent.ACTION_UP) {
            navigation = null
            navigationConsumed = false
        }
        return consumed
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        navigation = null
        cancelActiveContact()
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        val action = event.actionMasked
        if (handleNavigation(event)) return true
        if (action == MotionEvent.ACTION_CANCEL) {
            cancelActiveContact()
            return true
        }
        if (action == MotionEvent.ACTION_MOVE) {
            val activeId = reducer.currentPointerId ?: return true
            val index = event.findPointerIndex(activeId)
            if (index < 0) return true
            for (historyIndex in 0 until event.historySize) {
                moveContact(event.sampleAt(index, historyIndex))
            }
            moveContact(event.sampleAt(index, null))
            invalidate()
            return true
        }
        val index = event.actionIndex
        if (index !in 0 until event.pointerCount) return true
        val sample = event.sampleAt(index, null)
        if ((action == MotionEvent.ACTION_DOWN || action == MotionEvent.ACTION_POINTER_DOWN) &&
            (sample.x < 0f || sample.y < 0f || sample.x > width || sample.y > height)
        ) {
            return true
        }
        when (val result = reducer.handle(action, sample.pointerId, sample, event.isCanceledUp())) {
            is InputResult.Started -> {
                contactReady = inputReady
                holdConsumed = false
                stopLaser()
                activePoints.clear()
                latchedTool = activeTool
                latchedColor = if (activeTool == DrawTool.HIGHLIGHTER) {
                    (activeColor and 0x00FFFFFF) or 0x50000000
                } else activeColor
                latchedWidth = minOf(activeWidth * if (activeTool == DrawTool.HIGHLIGHTER) 4 else 1,
                    minOf(width, height) * 0.09f)
                val point = clampX(sample.x) to clampY(sample.y)
                activePoints.add(point)
                lastPointerPosition = point
                pointerButton = if (sample.buttonState and MotionEvent.BUTTON_STYLUS_PRIMARY != 0) "right" else "left"
                pointerDownSent = false
                hold.begin(point.first, point.second, SystemClock.uptimeMillis())
                postDelayed(holdTimer, 550)
                editTool = latchedTool
                resizeCorner = 2
                editStart = selection?.takeIf { selected ->
                    if (!isEditTool(latchedTool)) false else {
                        val frame = selectionFrame(selected)
                        val handle = frame.hitHandle(point.first, point.second, 24 * density,
                            keepMoveCenter = latchedTool == DrawTool.SELECT_MOVE)
                        if (handle != null) {
                            editTool = if (handle == -1) DrawTool.ROTATE else DrawTool.RESIZE
                            if (handle >= 0) resizeCorner = handle
                        }
                        handle != null || frame.contains(point.first, point.second, 12 * density)
                    }
                }
                editTransform = null
                if (contactReady && drawMode && latchedTool == DrawTool.LASER) updateLaser(point)
                invalidate()
            }
            is InputResult.Ended -> {
                removeCallbacks(holdTimer)
                val end = result.sample?.let { clampX(it.x) to clampY(it.y) } ?: lastPointerPosition
                hold.move(end.first, end.second)
                if (!result.canceled && hold.takeLongPress(SystemClock.uptimeMillis())) {
                    openPenMenu()
                } else if (!holdConsumed && contactReady && inputReady) {
                    if (!drawMode) finishPointer(end, result.canceled)
                    else if (!result.canceled) finishDrawing(end)
                }
                hold.cancel()
                stopLaser()
                activePoints.clear()
                editStart = null
                editTransform = null
                invalidate()
            }
            else -> Unit
        }
        return true
    }

    private fun moveContact(sample: PenSample) {
        if (reducer.handle(MotionEvent.ACTION_MOVE, sample.pointerId, sample) !is InputResult.Moved) return
        val point = clampX(sample.x) to clampY(sample.y)
        hold.move(point.first, point.second)
        if (!hold.pending) removeCallbacks(holdTimer)
        lastPointerPosition = point
        if (!contactReady || !inputReady) return
        if (!drawMode) {
            if (!hold.pending) {
                sendPointerDown()
                onPointerEvent?.invoke(PointerEvent("move", point.first, point.second, pointerButton))
            }
        } else if (latchedTool == DrawTool.LASER) updateLaser(point)
        else {
            if (activePoints.size >= 20_000) {
                cancelActiveContact()
                onMessage?.invoke("Stroke exceeds 20,000 samples; split it into smaller strokes.")
                return
            }
            activePoints.add(point)
            editTransform = editStart?.let { transformedSelection(it, point) }
        }
    }

    private fun openPenMenu() {
        val point = hold.origin
        holdConsumed = true
        hold.cancel()
        removeCallbacks(holdTimer)
        stopLaser()
        reducer.cancel()
        activePoints.clear()
        editStart = null
        editTransform = null
        onLongPress?.invoke(point.first, point.second)
        invalidate()
    }

    private fun sendPointerDown() {
        if (pointerDownSent) return
        val start = hold.origin
        onPointerEvent?.invoke(PointerEvent("down", start.first, start.second, pointerButton))
        pointerDownSent = true
    }

    private fun finishPointer(end: Pair<Float, Float>, canceled: Boolean) {
        if (!canceled) sendPointerDown()
        if (pointerDownSent) onPointerEvent?.invoke(
            PointerEvent(if (canceled) "cancel" else "up", end.first, end.second, pointerButton),
        )
        pointerDownSent = false
    }

    private fun finishDrawing(end: Pair<Float, Float>) {
        val start = activePoints.firstOrNull() ?: return
        val moved = hypot(end.first - start.first, end.second - start.second) >= 3f
        when {
            latchedTool == DrawTool.LASER -> Unit
            isEditTool(latchedTool) -> {
                val selected = editStart
                if (selected != null && moved) {
                    val transform = transformedSelection(selected, end)
                    pendingTransform = transform
                    if (onTransform?.invoke(selected.id, transform) != true) pendingTransform = null
                } else onSelect?.invoke(end.first / width, end.second / height)
            }
            latchedTool == DrawTool.TEXT -> onRequestText?.invoke(end.first / width, end.second / height)
            latchedTool == DrawTool.POLYGON || latchedTool == DrawTool.POLYLINE -> {
                if (vertices.size >= 1_000) {
                    onMessage?.invoke("Path has 1,000 vertices; finish it before adding more.")
                } else {
                    if (vertexStyle == null) vertexStyle = InkMark(latchedTool, latchedColor, latchedWidth, emptyList())
                    vertices.add(end)
                    onMessage?.invoke("${vertices.size} vertices; long-press > More > Finish path.")
                }
            }
            latchedTool != DrawTool.IMAGE -> {
                activePoints.add(end)
                if (latchedTool in listOf(DrawTool.FREEHAND, DrawTool.HIGHLIGHTER) || moved) {
                    val points = shapePoints(activePoints)
                    if (latchedTool == DrawTool.CIRCLE &&
                        hypot(points.last().first - points.first().first,
                            points.last().second - points.first().second) < 3f) {
                        onMessage?.invoke("Drag diagonally to define a circle.")
                    } else commit(InkMark(latchedTool, latchedColor, latchedWidth, points))
                }
            }
        }
    }

    fun finishPath() {
        val style = vertexStyle
        val minimum = if (style?.tool == DrawTool.POLYGON) 3 else 2
        if (style == null || vertices.size < minimum) {
            onMessage?.invoke("Add at least $minimum pen-tapped vertices first.")
            return
        }
        commit(style.copy(points = vertices.toList()))
        vertices.clear()
        vertexStyle = null
        invalidate()
    }

    fun addText(x: Float, y: Float, text: String, fontSize: Float) {
        val size = fontSize * minOf(width, height)
        paint.textSize = size
        val lines = text.lines()
        val right = minOf(width.toFloat(), x * width + lines.maxOf { paint.measureText(it) })
        val bottom = minOf(height.toFloat(), y * height + size * 1.25f * lines.size)
        commit(InkMark(DrawTool.TEXT, activeColor, size,
            listOf(x * width to y * height, right to bottom), text = text))
    }

    private fun commit(mark: InkMark) {
        if (!remoteVideoVisible) {
            if (marks.size >= 2_000) {
                onMessage?.invoke("Local preview is full; clear it before drawing more.")
                return
            }
            marks.add(mark)
            redoMarks.clear()
            onCommit?.invoke(mark)
        } else {
            if (!pendingInk.canAdd(mark)) {
                onMessage?.invoke("Waiting for PC ink; pending preview budget is full. Stroke was not sent.")
                return
            }
            val id = onCommit?.invoke(mark) ?: return
            val canonical = navigationViewport.mark(mark, width, height)
            val normalized = canonical.copy(width = canonical.width / minOf(width, height),
                points = canonical.points.map { it.first / width to it.second / height })
            val wasEmpty = pendingInk.isEmpty
            pendingInk.add(id, sourceEpoch, normalized, SystemClock.uptimeMillis())
            if (wasEmpty) {
                removeCallbacks(handoffTimer)
                postDelayed(handoffTimer, 500)
            }
        }
        invalidate()
    }

    private fun transformedSelection(selected: EditorSelection, end: Pair<Float, Float>): Affine2D {
        val start = activePoints.firstOrNull() ?: end
        val dx = (end.first - start.first) / width
        val dy = (end.second - start.second) / height
        return when (editTool) {
            DrawTool.RESIZE -> selected.resize(resizeCorner,
                start.first / width to start.second / height, end.first / width to end.second / height)
            DrawTool.ROTATE -> {
                val center = selected.center
                val cx = center.first * width
                val cy = center.second * height
                val angle = kotlin.math.atan2(end.second - cy, end.first - cx) -
                    kotlin.math.atan2(start.second - cy, start.first - cx)
                Affine2D.rotation(angle, width, height, center.first, center.second) * selected.transform
            }
            else -> Affine2D(tx = dx, ty = dy) * selected.transform
        }
    }

    private fun selectionFrame(selected: EditorSelection) =
        selected.frame(width, height, 48 * density, 16 * density,
            editTransform ?: pendingTransform ?: selected.transform)

    override fun onResolvePointerIcon(event: MotionEvent, pointerIndex: Int): PointerIcon? {
        val selected = selection
        if (!drawMode || !inputReady || selected == null || !isEditTool(activeTool) ||
            width == 0 || height == 0 || pointerIndex !in 0 until event.pointerCount) {
            return super.onResolvePointerIcon(event, pointerIndex)
        }
        val x = event.getX(pointerIndex)
        val y = event.getY(pointerIndex)
        val frame = selectionFrame(selected)
        val handle = frame.hitHandle(x, y, 24 * density, keepMoveCenter = activeTool == DrawTool.SELECT_MOVE)
        if (handle == -1 || (handle == null && activeTool == DrawTool.ROTATE && frame.contains(x, y, 12 * density))) {
            return rotationCursor
        }
        val corner = handle ?: if (activeTool == DrawTool.RESIZE && frame.contains(x, y, 12 * density)) 2 else null
        if (corner != null) {
            val angle = resizeAngle(frame, corner).let { (it + 180f) % 180f }
            val type = when {
                angle < 22.5f || angle >= 157.5f -> PointerIcon.TYPE_HORIZONTAL_DOUBLE_ARROW
                angle < 67.5f -> PointerIcon.TYPE_TOP_LEFT_DIAGONAL_DOUBLE_ARROW
                angle < 112.5f -> PointerIcon.TYPE_VERTICAL_DOUBLE_ARROW
                else -> PointerIcon.TYPE_TOP_RIGHT_DIAGONAL_DOUBLE_ARROW
            }
            return PointerIcon.getSystemIcon(context, type)
        }
        return if (frame.contains(x, y, 12 * density)) PointerIcon.getSystemIcon(context, PointerIcon.TYPE_ALL_SCROLL)
            else super.onResolvePointerIcon(event, pointerIndex)
    }

    private fun resizeAngle(frame: SelectionFrame, corner: Int): Float {
        val point = frame.corners[corner]
        val opposite = frame.corners[(corner + 2) % 4]
        return Math.toDegrees(kotlin.math.atan2(point.second - opposite.second,
            point.first - opposite.first).toDouble()).toFloat()
    }

    private fun isEditTool(tool: DrawTool) =
        tool in listOf(DrawTool.SELECT_MOVE, DrawTool.ROTATE, DrawTool.RESIZE)

    private fun updateLaser(point: Pair<Float, Float>) {
        laserPosition = point
        removeCallbacks(laserTimer)
        laserTimer.run()
    }

    private fun stopLaser() {
        removeCallbacks(laserTimer)
        laserPosition?.let { onLaser?.invoke(false, it.first, it.second, activeColor) }
        laserPosition = null
    }

    override fun onHoverEvent(event: MotionEvent): Boolean {
        if (event.pointerCount == 0) return false
        val toolType = event.getToolType(event.actionIndex.coerceIn(0, event.pointerCount - 1))
        if (toolType == MotionEvent.TOOL_TYPE_ERASER) {
            statusMessage = "ERASER NOT SUPPORTED - no ink created"
            invalidate()
            return true
        }
        if (inputReady && toolType == MotionEvent.TOOL_TYPE_STYLUS && reducer.currentPointerId == null) {
            val sample = event.sampleAt(event.actionIndex)
            if (!sample.x.isFinite() || !sample.y.isFinite()) return true
            lastPointerPosition = sample.x to sample.y
            if (drawMode && activeTool == DrawTool.LASER) {
                if (event.actionMasked == MotionEvent.ACTION_HOVER_EXIT) stopLaser()
                else updateLaser(clampX(sample.x) to clampY(sample.y))
            } else if (!drawMode && event.actionMasked != MotionEvent.ACTION_HOVER_EXIT) {
                onPointerEvent?.invoke(PointerEvent("hover", clampX(sample.x), clampY(sample.y), "left"))
            }
            return true
        }
        return toolType == MotionEvent.TOOL_TYPE_STYLUS
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        if (showRecovery && width > 0 && height > 0) {
            canvas.drawColor(android.graphics.Color.BLACK)
            if (recoveryCanvas.second) {
                paint.color = 0xFF444444.toInt()
                paint.strokeWidth = 1f
                for (x in 0..width step 32) canvas.drawLine(x.toFloat(), 0f, x.toFloat(), height.toFloat(), paint)
                for (y in 0..height step 32) canvas.drawLine(0f, y.toFloat(), width.toFloat(), y.toFloat(), paint)
            }
            for ((mark, normalizedMatrix) in recoveryObjects) {
                canvas.save()
                canvas.scale(width.toFloat(), height.toFloat())
                canvas.concat(normalizedMatrix)
                canvas.scale(1f / width, 1f / height)
                drawMark(canvas, mark.copy(width = mark.width * minOf(width, height),
                    points = mark.points.map { it.first * width to it.second * height }))
                canvas.restore()
            }
        }
        if (!remoteVideoVisible && !showRecovery) for (mark in marks) drawMark(canvas, mark)
        if (remoteVideoVisible && !showRecovery) {
            val view = displayedViewport
            for (mark in pendingInk.marks) {
                drawMark(canvas, mark.copy(width = mark.width * minOf(width, height) * view.scale,
                    points = mark.points.map {
                        (it.first - view.left) * view.scale * width to
                            (it.second - view.top) * view.scale * height
                    }))
            }
        }
        if (vertices.isNotEmpty()) {
            val style = vertexStyle
            if (style != null) drawMark(canvas, style.copy(
                points = vertices + activePoints.lastOrNull().let { if (it == null) emptyList() else listOf(it) },
            ))
        } else if (activePoints.isNotEmpty() && inputReady && contactReady && drawMode && !isEditTool(latchedTool)) {
            drawMark(canvas, InkMark(latchedTool, latchedColor, latchedWidth, shapePoints(activePoints)))
        }
        if (width > 0 && height > 0) selection?.let { drawSelection(canvas, selectionFrame(it)) }
        if (remoteVideoVisible && !showRecovery) return
        paint.style = Paint.Style.FILL
        paint.color = 0x99000000.toInt()
        canvas.drawRect(0f, 0f, width.toFloat(), 44f, paint)
        paint.color = 0xFFFFFFFF.toInt()
        paint.textSize = 14f * resources.displayMetrics.density
        canvas.drawText(
            statusMessage,
            12f,
            28f * resources.displayMetrics.density,
            paint,
        )
    }

    private fun drawSelection(canvas: Canvas, frame: SelectionFrame) {
        val path = Path().apply {
            moveTo(frame.corners[0].first, frame.corners[0].second)
            for ((x, y) in frame.corners.drop(1)) lineTo(x, y)
            close()
        }
        paint.style = Paint.Style.STROKE
        paint.color = 0xFFFFFFFF.toInt()
        paint.strokeWidth = 7 * density
        canvas.drawPath(path, paint)
        paint.color = selectionBlue
        paint.strokeWidth = 4 * density
        canvas.drawPath(path, paint)
        val topX = (frame.corners[0].first + frame.corners[1].first) / 2
        val topY = (frame.corners[0].second + frame.corners[1].second) / 2
        val (rx, ry) = frame.rotationHandle
        paint.color = 0xFFFFFFFF.toInt()
        paint.strokeWidth = 5 * density
        canvas.drawLine(topX, topY, rx, ry, paint)
        paint.color = selectionBlue
        paint.strokeWidth = 2 * density
        canvas.drawLine(topX, topY, rx, ry, paint)
        for ((index, point) in (frame.corners + frame.rotationHandle).withIndex()) {
            val (x, y) = point
            paint.style = Paint.Style.FILL
            paint.color = 0xFFFFFFFF.toInt()
            canvas.drawCircle(x, y, 14 * density, paint)
            paint.style = Paint.Style.STROKE
            paint.color = selectionBlue
            paint.strokeWidth = 2 * density
            canvas.drawCircle(x, y, 14 * density, paint)
            if (index == 4) drawRotationGlyph(canvas, paint, x, y, 7 * density) else {
                canvas.save()
                canvas.rotate(resizeAngle(frame, index), x, y)
                val length = 7 * density
                val head = 4 * density
                val arrow = Path().apply {
                    moveTo(x - length, y); lineTo(x + length, y)
                    moveTo(x - length + head, y - head); lineTo(x - length, y); lineTo(x - length + head, y + head)
                    moveTo(x + length - head, y - head); lineTo(x + length, y); lineTo(x + length - head, y + head)
                }
                canvas.drawPath(arrow, paint)
                canvas.restore()
            }
        }
    }

    private fun drawRotationGlyph(canvas: Canvas, targetPaint: Paint, x: Float, y: Float, radius: Float) {
        canvas.drawArc(x - radius, y - radius, x + radius, y + radius, -135f, 270f, false, targetPaint)
        val tipX = x - radius * 0.707107f
        val tipY = y + radius * 0.707107f
        val head = radius * 0.65f
        canvas.drawPath(Path().apply {
            moveTo(tipX + head, tipY)
            lineTo(tipX, tipY)
            lineTo(tipX, tipY + head)
        }, targetPaint)
    }

    private fun drawMark(canvas: Canvas, mark: InkMark) {
        if (mark.points.isEmpty()) return
        paint.color = mark.color
        paint.strokeWidth = mark.width
        paint.style = Paint.Style.STROKE
        val first = mark.points.first()
        when (mark.tool) {
            DrawTool.FREEHAND, DrawTool.HIGHLIGHTER, DrawTool.POLYGON, DrawTool.POLYLINE -> {
                if (mark.points.size == 1) {
                    paint.style = Paint.Style.FILL
                    canvas.drawCircle(first.first, first.second, max(1f, mark.width / 2f), paint)
                } else {
                    val path = Path().apply {
                        moveTo(first.first, first.second)
                        mark.points.drop(1).forEach { lineTo(it.first, it.second) }
                        if (mark.tool == DrawTool.POLYGON && mark.points.size >= 3) close()
                    }
                    canvas.drawPath(path, paint)
                }
            }
            DrawTool.ARROW, DrawTool.LINE -> {
                val end = mark.points.last()
                canvas.drawLine(first.first, first.second, end.first, end.second, paint)
                val angle = kotlin.math.atan2(end.second - first.second, end.first - first.first)
                val head = max(mark.width * 4f, 14f * resources.displayMetrics.density)
                for (offset in if (mark.tool == DrawTool.ARROW) listOf(-0.55f, 0.55f) else emptyList()) {
                    val a = angle + Math.PI.toFloat() + offset
                    canvas.drawLine(
                        end.first,
                        end.second,
                        end.first + kotlin.math.cos(a) * head,
                        end.second + kotlin.math.sin(a) * head,
                        paint,
                    )
                }
            }
            DrawTool.RECTANGLE -> {
                val end = mark.points.last()
                canvas.drawRect(
                    minOf(first.first, end.first), minOf(first.second, end.second),
                    maxOf(first.first, end.first), maxOf(first.second, end.second), paint,
                )
            }
            DrawTool.ELLIPSE, DrawTool.CIRCLE -> {
                val end = mark.points.last()
                canvas.drawOval(
                    minOf(first.first, end.first), minOf(first.second, end.second),
                    maxOf(first.first, end.first), maxOf(first.second, end.second), paint,
                )
            }
            DrawTool.TEXT -> {
                paint.style = Paint.Style.FILL
                paint.textSize = mark.width
                mark.text?.lines()?.forEachIndexed { index, line ->
                    canvas.drawText(line, first.first, first.second + mark.width * (1 + index * 1.25f), paint)
                }
            }
            DrawTool.IMAGE -> {
                mark.imageData?.let { data ->
                    val bitmap = requireNotNull(recoveryImages[data]) { "Saved image preview is missing" }
                    val end = mark.points.last()
                    canvas.drawBitmap(bitmap, null, android.graphics.RectF(first.first, first.second,
                        end.first, end.second), null)
                }
            }
            DrawTool.LASER, DrawTool.SELECT_MOVE, DrawTool.ROTATE, DrawTool.RESIZE -> Unit
        }
    }

    private fun shapePoints(points: List<Pair<Float, Float>>): List<Pair<Float, Float>> {
        if (latchedTool != DrawTool.CIRCLE || points.size < 2) return points.toList()
        val first = points.first()
        val end = points.last()
        val dx = end.first - first.first
        val dy = end.second - first.second
        val side = minOf(kotlin.math.abs(dx), kotlin.math.abs(dy))
        return listOf(first, first.first + kotlin.math.sign(dx) * side to first.second + kotlin.math.sign(dy) * side)
    }

    override fun onDetachedFromWindow() {
        cancelActiveContact()
        super.onDetachedFromWindow()
    }

    private fun MotionEvent.sampleAt(index: Int): PenSample = sampleAt(index, null)

    private fun MotionEvent.sampleAt(index: Int, history: Int?): PenSample {
        val toolType = getToolType(index)
        if (toolType == MotionEvent.TOOL_TYPE_ERASER) {
            statusMessage = "ERASER NOT SUPPORTED - no ink created"
        }
        val pressure = getAxisValue(MotionEvent.AXIS_PRESSURE, index, history)
            .takeIf { it.isFinite() && toolType != MotionEvent.TOOL_TYPE_FINGER }
        return PenSample(
            pointerId = getPointerId(index),
            toolType = toolType,
            x = if (history == null) getX(index) else getHistoricalX(index, history),
            y = if (history == null) getY(index) else getHistoricalY(index, history),
            eventTimeUs = (if (history == null) eventTime else getHistoricalEventTime(history)) * 1_000,
            pressure = pressure,
            tilt = getAxisValue(MotionEvent.AXIS_TILT, index, history).takeIf { it.isFinite() },
            orientation = getAxisValue(MotionEvent.AXIS_ORIENTATION, index, history)
                .takeIf { it.isFinite() },
            buttonState = buttonState,
        )
    }

    private fun MotionEvent.getAxisValue(axis: Int, pointer: Int, history: Int?): Float =
        if (history == null) getAxisValue(axis, pointer) else getHistoricalAxisValue(axis, pointer, history)

    private fun clampX(value: Float) = value.coerceIn(0f, width.toFloat())
    private fun clampY(value: Float) = value.coerceIn(0f, height.toFloat())

    private fun MotionEvent.isCanceledUp(): Boolean =
        android.os.Build.VERSION.SDK_INT >= 33 &&
            (actionMasked == MotionEvent.ACTION_UP ||
                actionMasked == MotionEvent.ACTION_POINTER_UP) &&
            flags and MotionEvent.FLAG_CANCELED != 0
}
