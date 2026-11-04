package com.drawdock

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.drawable.GradientDrawable
import android.view.Gravity
import android.view.View
import android.widget.Button
import android.widget.FrameLayout
import kotlin.math.cos
import kotlin.math.sin

class RadialMenuView(context: Context) : FrameLayout(context) {
    data class Item(val label: String, val action: () -> Unit)
    private val density = resources.displayMetrics.density
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = 0xEE202830.toInt() }
    private var anchorX = 0f
    private var anchorY = 0f
    private var centerX = 0f
    private var centerY = 0f
    private var radius = 0f
    private var items = emptyList<Item>()
    var onDismiss: (() -> Unit)? = null

    init {
        visibility = View.GONE
        elevation = 24 * density
        setWillNotDraw(false)
        setOnClickListener { dismiss() }
        contentDescription = "Drawing radial menu"
    }

    fun open(x: Float, y: Float) {
        anchorX = x
        anchorY = y
        visibility = View.VISIBLE
        bringToFront()
    }

    fun page(entries: List<Item>, back: (() -> Unit)? = null) {
        items = entries
        removeAllViews()
        entries.forEach { entry ->
            addView(makeButton(entry.label) { entry.action() })
        }
        addView(makeButton(if (back == null) "Close" else "Back") {
            if (back == null) dismiss() else back()
        })
        requestLayout()
        invalidate()
    }

    fun dismiss() {
        if (visibility != View.VISIBLE) return
        visibility = View.GONE
        onDismiss?.invoke()
    }

    private fun makeButton(label: String, action: () -> Unit) = Button(context).apply {
        text = label
        textSize = 11f
        isAllCaps = false
        gravity = Gravity.CENTER
        setTextColor(Color.WHITE)
        setPadding(2, 2, 2, 2)
        background = GradientDrawable().apply {
            shape = GradientDrawable.OVAL
            setColor(0xFF36536B.toInt())
            setStroke((density * 1.5f).toInt(), 0xFF84B7DD.toInt())
        }
        contentDescription = label.replace('\n', ' ')
        setOnClickListener { action() }
    }

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        super.onMeasure(widthMeasureSpec, heightMeasureSpec)
        val size = buttonSize(measuredWidth, measuredHeight)
        val spec = MeasureSpec.makeMeasureSpec(size, MeasureSpec.EXACTLY)
        for (index in 0 until childCount) getChildAt(index).measure(spec, spec)
    }

    private fun buttonSize(w: Int, h: Int) =
        minOf(66 * density, minOf(350 * density, w.toFloat(), h.toFloat()) / 4.8f).toInt().coerceAtLeast(1)

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        if (width <= 0 || height <= 0 || childCount == 0) return
        val diameter = minOf(350 * density, width.toFloat(), height.toFloat())
        val buttonSize = buttonSize(width, height)
        val outer = diameter / 2
        radius = (outer - buttonSize / 2 - 4 * density).coerceAtLeast(0f)
        centerX = anchorX.coerceIn(outer, maxOf(outer, width - outer))
        centerY = anchorY.coerceIn(outer, maxOf(outer, height - outer))
        for (index in 0 until childCount) {
            val angle = -Math.PI / 2 + index * 2 * Math.PI / items.size.coerceAtLeast(1)
            val x = centerX + if (index == items.size) 0f else (cos(angle) * radius).toFloat()
            val y = centerY + if (index == items.size) 0f else (sin(angle) * radius).toFloat()
            val childLeft = (x - buttonSize / 2).toInt()
            val childTop = (y - buttonSize / 2).toInt()
            getChildAt(index).layout(childLeft, childTop, childLeft + buttonSize, childTop + buttonSize)
        }
    }

    override fun onDraw(canvas: Canvas) {
        canvas.drawCircle(centerX, centerY, radius + 34 * density, paint)
        super.onDraw(canvas)
    }
}
