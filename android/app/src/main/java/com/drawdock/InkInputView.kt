package com.drawdock

import android.content.Context
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.view.MotionEvent
import android.view.View
import kotlin.math.max
import kotlin.math.hypot

data class InkMark(
    val tool: DrawTool,
    val color: Int,
    val width: Float,
    val points: List<Pair<Float, Float>>,
) {
    fun commitPoints(): List<Pair<Float, Float>> =
        if (tool == DrawTool.FREEHAND || points.size < 2) points
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
    private var pointerButton = "left"
    private var lastPointerPosition = 0f to 0f
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
        if (reducer.currentPointerId != null &&
            (activeTool != tool || activeColor != color || activeWidth != width)
        ) {
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
        invalidate()
    }

    fun cancelActiveContact() {
        if (!drawMode && reducer.currentPointerId != null) {
            onPointerEvent?.invoke(
                PointerEvent(
                    "cancel",
                    clampX(lastPointerPosition.first),
                    clampY(lastPointerPosition.second),
                    pointerButton,
                ),
            )
        }
        reducer.cancel()
        activePoints.clear()
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
        if (!inputReady) return true
        if (!drawMode) return handlePointerTouch(event)
        val action = event.actionMasked
        if (action == MotionEvent.ACTION_CANCEL) {
            val result = reducer.handle(action, -1, null)
            if (result is InputResult.Ended) {
                activePoints.clear()
                invalidate()
            }
            return true
        }

        if (action == MotionEvent.ACTION_MOVE) {
            val activeId = reducer.currentPointerId ?: return true
            val index = event.findPointerIndex(activeId)
            if (index < 0) return true
            for (historyIndex in 0 until event.historySize) {
                val historical = event.sampleAt(index, historyIndex)
                if (reducer.handle(MotionEvent.ACTION_MOVE, activeId, historical) is InputResult.Moved) {
                    activePoints.add(clampX(historical.x) to clampY(historical.y))
                }
            }
            val current = event.sampleAt(index, null)
            if (reducer.handle(MotionEvent.ACTION_MOVE, activeId, current) is InputResult.Moved) {
                activePoints.add(clampX(current.x) to clampY(current.y))
            }
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
                activePoints.clear()
                latchedTool = activeTool
                latchedColor = activeColor
                latchedWidth = activeWidth
                activePoints.add(clampX(result.sample.x) to clampY(result.sample.y))
                invalidate()
            }
            is InputResult.Ended -> {
                if (!result.canceled && activePoints.isNotEmpty()) {
                    result.sample?.let { activePoints.add(clampX(it.x) to clampY(it.y)) }
                    val mark = InkMark(
                        latchedTool,
                        latchedColor,
                        latchedWidth,
                        activePoints.toList(),
                    )
                    val validShape = mark.tool == DrawTool.FREEHAND ||
                        (mark.points.size >= 2 && hypot(
                            mark.points.last().first - mark.points.first().first,
                            mark.points.last().second - mark.points.first().second,
                        ) >= 3f)
                    if (validShape) {
                        marks.add(mark)
                        redoMarks.clear()
                        onCommit?.invoke(mark)
                    }
                }
                activePoints.clear()
                invalidate()
            }
            else -> Unit
        }
        return true
    }

    override fun onHoverEvent(event: MotionEvent): Boolean {
        if (event.pointerCount == 0) return false
        val toolType = event.getToolType(event.actionIndex.coerceIn(0, event.pointerCount - 1))
        if (toolType == MotionEvent.TOOL_TYPE_ERASER) {
            statusMessage = "ERASER NOT SUPPORTED - no ink created"
            invalidate()
            return true
        }
        if (!drawMode && inputReady && toolType == MotionEvent.TOOL_TYPE_STYLUS) {
            val sample = event.sampleAt(event.actionIndex)
            lastPointerPosition = sample.x to sample.y
            onPointerEvent?.invoke(PointerEvent("hover", clampX(sample.x), clampY(sample.y), "left"))
            return true
        }
        return toolType == MotionEvent.TOOL_TYPE_STYLUS
    }

    private fun handlePointerTouch(event: MotionEvent): Boolean {
        val action = event.actionMasked
        if (action == MotionEvent.ACTION_CANCEL) {
            if (reducer.currentPointerId != null) {
                onPointerEvent?.invoke(
                    PointerEvent(
                        "cancel",
                        clampX(lastPointerPosition.first),
                        clampY(lastPointerPosition.second),
                        pointerButton,
                    ),
                )
            }
            reducer.handle(action, -1, null)
            return true
        }
        if (action == MotionEvent.ACTION_MOVE) {
            val pointerId = reducer.currentPointerId ?: return true
            val index = event.findPointerIndex(pointerId)
            if (index < 0) return true
            val sample = event.sampleAt(index, null)
            if (reducer.handle(action, pointerId, sample) is InputResult.Moved) {
                lastPointerPosition = sample.x to sample.y
                onPointerEvent?.invoke(
                    PointerEvent("move", clampX(sample.x), clampY(sample.y), pointerButton),
                )
            }
            return true
        }
        val index = event.actionIndex
        if (index !in 0 until event.pointerCount) return true
        val sample = event.sampleAt(index, null)
        when (val result = reducer.handle(action, sample.pointerId, sample, event.isCanceledUp())) {
            is InputResult.Started -> {
                pointerButton = if (sample.buttonState and MotionEvent.BUTTON_STYLUS_PRIMARY != 0) {
                    "right"
                } else {
                    "left"
                }
                lastPointerPosition = sample.x to sample.y
                onPointerEvent?.invoke(
                    PointerEvent("down", clampX(sample.x), clampY(sample.y), pointerButton),
                )
            }
            is InputResult.Ended -> {
                val finalPoint = result.sample?.let { it.x to it.y } ?: lastPointerPosition
                lastPointerPosition = finalPoint
                onPointerEvent?.invoke(
                    PointerEvent(
                        if (result.canceled) "cancel" else "up",
                        clampX(finalPoint.first),
                        clampY(finalPoint.second),
                        pointerButton,
                    ),
                )
            }
            else -> Unit
        }
        return true
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        if (!remoteVideoVisible) for (mark in marks) drawMark(canvas, mark)
        if (activePoints.isNotEmpty()) {
            drawMark(canvas, InkMark(latchedTool, latchedColor, latchedWidth, activePoints))
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
            DrawTool.FREEHAND -> {
                if (mark.points.size == 1) {
                    paint.style = Paint.Style.FILL
                    canvas.drawCircle(first.first, first.second, max(1f, mark.width / 2f), paint)
                } else {
                    val path = Path().apply {
                        moveTo(first.first, first.second)
                        mark.points.drop(1).forEach { lineTo(it.first, it.second) }
                    }
                    canvas.drawPath(path, paint)
                }
            }
            DrawTool.ARROW -> {
                val end = mark.points.last()
                canvas.drawLine(first.first, first.second, end.first, end.second, paint)
                val angle = kotlin.math.atan2(end.second - first.second, end.first - first.first)
                val head = max(mark.width * 4f, 14f * resources.displayMetrics.density)
                for (offset in listOf(-0.55f, 0.55f)) {
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
            DrawTool.ELLIPSE -> {
                val end = mark.points.last()
                canvas.drawOval(
                    minOf(first.first, end.first), minOf(first.second, end.second),
                    maxOf(first.first, end.first), maxOf(first.second, end.second), paint,
                )
            }
        }
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
