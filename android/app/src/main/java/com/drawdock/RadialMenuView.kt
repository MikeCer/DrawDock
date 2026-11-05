package com.drawdock

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.ColorFilter
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PixelFormat
import android.graphics.Typeface
import android.graphics.drawable.Drawable
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.StateListDrawable
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.widget.Button
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.ScrollView
import kotlin.math.ceil
import kotlin.math.roundToInt

class RadialMenuView(context: Context) : FrameLayout(context) {
    data class Item(
        val label: String,
        val icon: Int,
        val description: String = label.replace('\n', ' '),
        val selected: Boolean = false,
        val activeFamily: Boolean = false,
        val toggleOn: Boolean? = null,
        val danger: Boolean = false,
        val swatch: Int? = null,
        val strokeWidth: Float? = null,
        val previewColor: Int? = null,
        val action: () -> Unit,
    )

    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private var anchorX = 0f
    private var anchorY = 0f
    private var ring: RadialMenuGeometry? = null
    private var items = emptyList<Item>()
    private var buttons = emptyList<Button>()
    private var listParams = emptyList<LinearLayout.LayoutParams>()
    private var listMode = false
    private val list = LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
    private val overflow = ScrollView(context).apply {
        addView(list)
        isClickable = true
        background = surface(color(R.color.dd_menu_surface), color(R.color.dd_menu_rim))
            .apply { shape = GradientDrawable.RECTANGLE }
        visibility = View.GONE
    }
    var onDismiss: (() -> Unit)? = null

    init {
        visibility = View.GONE
        elevation = dimen(R.dimen.dd_menu_elevation)
        setWillNotDraw(false)
        setOnClickListener { dismiss() }
        contentDescription = "Drawing radial menu"
    }

    private fun dimen(id: Int) = resources.getDimension(id)
    private fun color(id: Int) = context.getColor(id)

    fun open(x: Float, y: Float) {
        anchorX = x
        anchorY = y
        visibility = View.VISIBLE
        bringToFront()
        requestLayout()
    }

    fun page(entries: List<Item>, back: (() -> Unit)? = null) {
        require(entries.size in 1..8)
        items = entries
        removeAllViews()
        list.removeAllViews()
        listMode = false
        overflow.visibility = View.GONE
        val center = Item(
            if (back == null) "Close" else "Back",
            if (back == null) R.drawable.ic_dd_close else R.drawable.ic_dd_back,
        ) { if (back == null) dismiss() else back() }
        buttons = (entries + center).mapIndexed { index, entry ->
            makeButton(entry, center = index == entries.size)
        }
        listParams = buttons.map { LinearLayout.LayoutParams(-1, -2) }
        buttons.forEach { addView(it) }
        addView(overflow)
        requestLayout()
        invalidate()
    }

    fun dismiss() {
        if (visibility != View.VISIBLE) return
        visibility = View.GONE
        onDismiss?.invoke()
    }

    private fun surface(fill: Int, stroke: Int = Color.TRANSPARENT) = GradientDrawable().apply {
        shape = GradientDrawable.OVAL
        setColor(fill)
        setStroke(ceil(dimen(R.dimen.dd_menu_rim_width)).toInt(), stroke)
    }

    private fun makeButton(entry: Item, center: Boolean) = Button(context).apply {
        text = entry.label
        setTextSize(TypedValue.COMPLEX_UNIT_PX, dimen(R.dimen.dd_menu_label))
        typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
        isAllCaps = false
        isSingleLine = false
        gravity = Gravity.CENTER
        includeFontPadding = false
        minWidth = 0
        minHeight = dimen(R.dimen.dd_menu_target).roundToInt()
        minimumWidth = 0
        minimumHeight = minHeight
        val accent = entry.selected || entry.toggleOn == true
        setTextColor(color(when {
            entry.danger -> R.color.dd_menu_danger
            accent -> R.color.dd_menu_accent
            else -> R.color.dd_menu_text
        }))
        val padding = dimen(R.dimen.dd_menu_item_padding).roundToInt()
        setPadding(padding, padding, padding, padding)
        background = StateListDrawable().apply {
            addState(intArrayOf(android.R.attr.state_pressed), surface(color(R.color.dd_menu_pressed)))
            addState(intArrayOf(android.R.attr.state_hovered), surface(color(R.color.dd_menu_hover)))
            addState(intArrayOf(android.R.attr.state_focused),
                surface(color(R.color.dd_menu_hover), color(R.color.dd_menu_accent)))
            addState(intArrayOf(), surface(
                if (center) color(R.color.dd_menu_center) else Color.TRANSPARENT,
                if (center) color(R.color.dd_menu_rim) else Color.TRANSPARENT,
            ))
        }
        stateListAnimator = null
        val preview = ItemPreview(entry)
        val size = dimen(R.dimen.dd_menu_preview).roundToInt()
        preview.setBounds(0, 0, size, size)
        setCompoundDrawables(null, preview, null, null)
        compoundDrawablePadding = dimen(R.dimen.dd_menu_label_gap).roundToInt()
        contentDescription = entry.description
        tooltipText = entry.description
        isSelected = entry.selected
        stateDescription = when {
            entry.toggleOn != null -> if (entry.toggleOn) "On" else "Off"
            entry.activeFamily -> "Current tool family"
            else -> null
        }
        setOnClickListener { entry.action() }
    }

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        super.onMeasure(widthMeasureSpec, heightMeasureSpec)
        if (buttons.isEmpty()) return
        val unspecified = MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED)
        buttons.forEach { it.measure(unspecified, unspecified) }
        val slot = maxOf(
            dimen(R.dimen.dd_menu_target).roundToInt(),
            buttons.maxOf { maxOf(it.measuredWidth, it.measuredHeight) },
        )
        val edge = dimen(R.dimen.dd_menu_edge_margin)
        ring = RadialMenuGeometry.fit(
            measuredWidth, measuredHeight, anchorX, anchorY, slot,
            dimen(R.dimen.dd_menu_diameter), edge,
        )
        val needsList = ring == null
        if (needsList != listMode) {
            buttons.forEachIndexed { index, button ->
                if (needsList) {
                    removeView(button)
                    list.addView(button, listParams[index])
                } else {
                    list.removeView(button)
                    addView(button)
                }
            }
            listMode = needsList
            overflow.visibility = if (needsList) View.VISIBLE else View.GONE
        }
        if (needsList) {
            val w = (measuredWidth - edge * 2).toInt().coerceAtLeast(1)
            val h = (measuredHeight - edge * 2).toInt().coerceAtLeast(1)
            overflow.measure(MeasureSpec.makeMeasureSpec(w, MeasureSpec.EXACTLY),
                MeasureSpec.makeMeasureSpec(h, MeasureSpec.EXACTLY))
        } else {
            val spec = MeasureSpec.makeMeasureSpec(slot, MeasureSpec.EXACTLY)
            buttons.forEach { it.measure(spec, spec) }
        }
    }

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        val geometry = ring
        if (geometry == null) {
            val edge = dimen(R.dimen.dd_menu_edge_margin).roundToInt()
            overflow.layout(edge, edge, edge + overflow.measuredWidth, edge + overflow.measuredHeight)
        } else {
            buttons.forEachIndexed { index, button ->
                val (x, y) = if (index == items.size) geometry.centerX to geometry.centerY
                    else geometry.itemCenter(index, items.size)
                val l = (x - geometry.slot / 2f).roundToInt()
                val t = (y - geometry.slot / 2f).roundToInt()
                button.layout(l, t, l + geometry.slot, t + geometry.slot)
            }
        }
    }

    override fun onDraw(canvas: Canvas) {
        ring?.let {
            paint.style = Paint.Style.FILL
            paint.color = color(R.color.dd_menu_surface)
            canvas.drawCircle(it.centerX, it.centerY, it.outerRadius, paint)
            paint.style = Paint.Style.STROKE
            paint.strokeWidth = dimen(R.dimen.dd_menu_rim_width)
            paint.color = color(R.color.dd_menu_rim)
            canvas.drawCircle(it.centerX, it.centerY, it.outerRadius, paint)
            paint.color = color(R.color.dd_menu_divider)
            canvas.drawCircle(it.centerX, it.centerY,
                it.slot / 2f + dimen(R.dimen.dd_menu_edge_margin), paint)
        }
        super.onDraw(canvas)
    }

    private inner class ItemPreview(private val entry: Item) : Drawable() {
        private val icon = if (entry.swatch == null && entry.strokeWidth == null)
            requireNotNull(context.getDrawable(entry.icon)).mutate() else null
        private val previewPaint = Paint(Paint.ANTI_ALIAS_FLAG)
        private val checkmark = Path().apply {
            val center = dimen(R.dimen.dd_menu_preview) / 2
            val radius = dimen(R.dimen.dd_menu_swatch) / 2
            moveTo(center - radius * 0.45f, center)
            lineTo(center - radius * 0.1f, center + radius * 0.35f)
            lineTo(center + radius * 0.5f, center - radius * 0.4f)
        }
        private var opacity = 255

        init {
            icon?.setTint(color(when {
                entry.selected -> R.color.dd_menu_on_accent
                entry.toggleOn == true -> R.color.dd_menu_accent
                entry.danger -> R.color.dd_menu_danger
                else -> R.color.dd_menu_text
            }))
        }

        override fun draw(canvas: Canvas) {
            val saved = if (opacity < 255) canvas.saveLayerAlpha(null, opacity) else canvas.save()
            canvas.translate(bounds.left.toFloat(), bounds.top.toFloat())
            val cx = bounds.width() / 2f
            val cy = bounds.height() / 2f
            val selectedRadius = dimen(R.dimen.dd_menu_selected_disc) / 2
            if (entry.selected) {
                previewPaint.color = color(R.color.dd_menu_accent)
                previewPaint.style = if (entry.swatch != null || entry.strokeWidth != null)
                    Paint.Style.STROKE else Paint.Style.FILL
                previewPaint.strokeWidth = dimen(R.dimen.dd_menu_selected_ring)
                canvas.drawCircle(cx, cy, selectedRadius, previewPaint)
            }
            previewPaint.style = Paint.Style.FILL
            val swatch = entry.swatch
            val strokeWidth = entry.strokeWidth
            if (swatch != null) {
                val radius = dimen(R.dimen.dd_menu_swatch) / 2
                previewPaint.color = swatch
                canvas.drawCircle(cx, cy, radius, previewPaint)
                previewPaint.style = Paint.Style.STROKE
                previewPaint.strokeWidth = dimen(R.dimen.dd_menu_swatch_outline_width)
                previewPaint.color = color(R.color.dd_menu_swatch_outline)
                canvas.drawCircle(cx, cy, radius, previewPaint)
                if (entry.selected) {
                    previewPaint.strokeWidth = dimen(R.dimen.dd_menu_check_width)
                    previewPaint.strokeCap = Paint.Cap.ROUND
                    previewPaint.strokeJoin = Paint.Join.ROUND
                    previewPaint.color = color(if (swatch == Color.YELLOW || swatch == Color.WHITE)
                        R.color.dd_menu_on_accent else R.color.dd_menu_text)
                    canvas.drawPath(checkmark, previewPaint)
                }
            } else if (strokeWidth != null) {
                val half = dimen(R.dimen.dd_menu_width_preview) / 2
                previewPaint.strokeCap = Paint.Cap.ROUND
                previewPaint.strokeWidth = strokeWidth + dimen(R.dimen.dd_menu_swatch_outline_width) * 2
                previewPaint.color = color(R.color.dd_menu_swatch_outline)
                canvas.drawLine(cx - half, cy, cx + half, cy, previewPaint)
                previewPaint.strokeWidth = strokeWidth
                previewPaint.color = requireNotNull(entry.previewColor)
                canvas.drawLine(cx - half, cy, cx + half, cy, previewPaint)
            } else {
                val half = dimen(R.dimen.dd_menu_icon) / 2
                icon?.setBounds((cx - half).roundToInt(), (cy - half).roundToInt(),
                    (cx + half).roundToInt(), (cy + half).roundToInt())
                icon?.draw(canvas)
            }
            if (entry.activeFamily || entry.previewColor != null && strokeWidth == null) {
                previewPaint.style = Paint.Style.FILL
                val radius = dimen(R.dimen.dd_menu_active_dot) / 2
                previewPaint.color = color(R.color.dd_menu_swatch_outline)
                canvas.drawCircle(cx, bounds.height() - radius,
                    radius + dimen(R.dimen.dd_menu_rim_width), previewPaint)
                previewPaint.color = entry.previewColor ?: color(R.color.dd_menu_accent)
                canvas.drawCircle(cx, bounds.height() - radius, radius, previewPaint)
            }
            if (entry.toggleOn == true) {
                previewPaint.style = Paint.Style.FILL
                previewPaint.textSize = dimen(R.dimen.dd_menu_toggle_label)
                previewPaint.typeface = Typeface.DEFAULT_BOLD
                val padding = dimen(R.dimen.dd_menu_label_gap)
                val w = maxOf(dimen(R.dimen.dd_menu_toggle_width), previewPaint.measureText("ON") + padding * 2)
                val h = maxOf(dimen(R.dimen.dd_menu_toggle_height),
                    previewPaint.descent() - previewPaint.ascent() + padding)
                val l = bounds.width() - w
                previewPaint.color = color(R.color.dd_menu_accent)
                canvas.drawRoundRect(l, 0f, bounds.width().toFloat(), h, padding, padding, previewPaint)
                previewPaint.color = color(R.color.dd_menu_on_accent)
                canvas.drawText("ON", l + (w - previewPaint.measureText("ON")) / 2,
                    (h - previewPaint.descent() - previewPaint.ascent()) / 2, previewPaint)
            }
            canvas.restoreToCount(saved)
        }

        override fun setAlpha(alpha: Int) { opacity = alpha; invalidateSelf() }
        override fun setColorFilter(colorFilter: ColorFilter?) {
            icon?.colorFilter = colorFilter
            previewPaint.colorFilter = colorFilter
            invalidateSelf()
        }
        @Suppress("OVERRIDE_DEPRECATION")
        override fun getOpacity() = PixelFormat.TRANSLUCENT
    }
}
