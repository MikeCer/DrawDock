package com.drawdock

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.os.SystemClock
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
    private val activePoints = mutableListOf<Pair<Float, Float>>()
    private var activeTool = DrawTool.FREEHAND
    private var activeColor = 0xFFFF0000.toInt()
    private var activeWidth = 6f
    private var latchedTool = activeTool
    private var latchedColor = activeColor
    private var latchedWidth = activeWidth
    private var statusMessage = "LOCAL PREVIEW ONLY - not connected to the PC"
    var onCommit: ((InkMark) -> Unit)? = null
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
        set(value) { field = value; invalidate() }
    private var editStart: EditorSelection? = null
    private var editDelta: Affine2D? = null
    private var laserPosition: Pair<Float, Float>? = null
    var onLongPress: ((Float, Float) -> Unit)? = null
    var onSelect: ((Float, Float) -> Unit)? = null
    var onTransform: ((Long, Affine2D) -> Unit)? = null
    var onRequestText: ((Float, Float) -> Unit)? = null
    var onLaser: ((Boolean, Float, Float, Int) -> Unit)? = null
    var onMessage: ((String) -> Unit)? = null
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
        selection = null
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
        editDelta = null
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

    override fun onTouchEvent(event: MotionEvent): Boolean {
        val action = event.actionMasked
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
                editStart = selection?.takeIf {
                    isEditTool(latchedTool) && it.bounds.contains(point.first / width, point.second / height)
                }
                editDelta = null
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
                editDelta = null
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
            editDelta = editStart?.let { transformDelta(it, point) }
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
        editDelta = null
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
                    onTransform?.invoke(selected.id, transformDelta(selected, end) * selected.transform)
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
        }
        onCommit?.invoke(mark)
        invalidate()
    }

    private fun transformDelta(selected: EditorSelection, end: Pair<Float, Float>): Affine2D {
        val start = activePoints.firstOrNull() ?: end
        val bounds = selected.bounds
        val dx = (end.first - start.first) / width
        val dy = (end.second - start.second) / height
        return when (latchedTool) {
            DrawTool.RESIZE -> Affine2D.around(bounds.left, bounds.top,
                (1 + dx / (bounds.right - bounds.left).coerceAtLeast(0.01f)).coerceIn(0.05f, 8f),
                0f, 0f, (1 + dy / (bounds.bottom - bounds.top).coerceAtLeast(0.01f)).coerceIn(0.05f, 8f))
            DrawTool.ROTATE -> {
                val cx = bounds.centerX * width
                val cy = bounds.centerY * height
                val angle = kotlin.math.atan2(end.second - cy, end.first - cx) -
                    kotlin.math.atan2(start.second - cy, start.first - cx)
                Affine2D.rotation(angle, width, height, bounds.centerX, bounds.centerY)
            }
            else -> Affine2D(tx = dx, ty = dy)
        }
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
        if (!remoteVideoVisible) for (mark in marks) drawMark(canvas, mark)
        if (vertices.isNotEmpty()) {
            val style = vertexStyle
            if (style != null) drawMark(canvas, style.copy(
                points = vertices + activePoints.lastOrNull().let { if (it == null) emptyList() else listOf(it) },
            ))
        } else if (activePoints.isNotEmpty() && inputReady && contactReady && drawMode && !isEditTool(latchedTool)) {
            drawMark(canvas, InkMark(latchedTool, latchedColor, latchedWidth, shapePoints(activePoints)))
        }
        selection?.let { selected ->
            val bounds = editDelta?.map(selected.bounds) ?: selected.bounds
            paint.style = Paint.Style.STROKE
            paint.strokeWidth = 2 * resources.displayMetrics.density
            paint.color = 0xFF0099FF.toInt()
            canvas.drawRect(bounds.left * width, bounds.top * height,
                bounds.right * width, bounds.bottom * height, paint)
            paint.style = Paint.Style.FILL
            for ((x, y) in listOf(bounds.left to bounds.top, bounds.right to bounds.top,
                bounds.left to bounds.bottom, bounds.right to bounds.bottom)) {
                canvas.drawCircle(x * width, y * height, 4 * resources.displayMetrics.density, paint)
            }
        }
        if (remoteVideoVisible) return
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
            DrawTool.IMAGE, DrawTool.LASER, DrawTool.SELECT_MOVE, DrawTool.ROTATE, DrawTool.RESIZE -> Unit
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
