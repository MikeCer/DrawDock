package com.drawdock

import android.content.Intent
import android.graphics.Color
import android.os.Bundle
import android.view.Gravity
import android.view.ViewGroup
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import android.text.InputType
import androidx.activity.ComponentActivity
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions

class MainActivity : ComponentActivity() {
    private val qrScanner = registerForActivityResult(ScanContract()) { result ->
        handleQrResult(result.contents)
    }
    private lateinit var inputView: InkInputView
    private lateinit var modeButton: Button
    private lateinit var connectionButton: Button
    private lateinit var clearPcButton: Button
    private lateinit var statusText: TextView
    private var sessionClient: SessionClient? = null
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
        }

        val controls = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(8, 8, 8, 8)
        }
        val topRow = row(controls)
        modeButton = button("Draw: ON") {
            requestModeChange()
        }.also(topRow::addView)
        connectionButton = button("Connect") {
            if (sessionClient == null) showConnectionDialog() else disconnectSession()
        }.also(topRow::addView)
        listOf(
            "Freehand" to DrawTool.FREEHAND,
            "Arrow" to DrawTool.ARROW,
            "Rectangle" to DrawTool.RECTANGLE,
            "Ellipse" to DrawTool.ELLIPSE,
        ).forEach { (label, tool) ->
            button(label) {
                selectedTool = tool
                configureInput()
            }.also(topRow::addView)
        }

        val secondRow = row(controls)
        listOf(
            "Red" to Color.RED,
            "Orange" to 0xFFFF9800.toInt(),
            "Yellow" to Color.YELLOW,
            "Green" to Color.GREEN,
            "Blue" to Color.BLUE,
            "Black" to Color.BLACK,
        ).forEach { (label, color) ->
            button(label) {
                selectedColor = color
                configureInput()
            }.also(secondRow::addView)
        }

        val thirdRow = row(controls)
        listOf("Thin" to 3f, "Medium" to 6f, "Thick" to 12f).forEach { (label, width) ->
            button(label) {
                selectedWidth = width * resources.displayMetrics.density
                configureInput()
            }.also(thirdRow::addView)
        }
        button("Undo") { inputView.undo() }.also(thirdRow::addView)
        button("Redo") { inputView.redo() }.also(thirdRow::addView)
        clearPcButton = button("Clear") {
            confirmRemoteClear()
        }.also(thirdRow::addView)

        statusText = TextView(this).apply {
            text = "Not connected; tablet preview alone is not PC annotation."
            setTextColor(Color.DKGRAY)
            textSize = 12f
        }
        controls.addView(statusText)
        val scroll = ScrollView(this).apply { addView(controls) }
        root.addView(
            scroll,
            LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT,
            ),
        )
        root.addView(
            inputView,
            LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                0,
                1f,
            ),
        )
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

    private fun requestModeChange() {
        val client = sessionClient
        if (client == null) {
            statusText.text = "Connect to the host before enabling Pointer mode."
            return
        }
        inputView.cancelActiveContact()
        inputView.setInputReady(false)
        val mode = if (inputView.drawMode) "pointer" else "draw"
        val requestId = client.requestMode(mode)
        if (requestId == null) {
            inputView.setInputReady(true)
            statusText.text = "Mode request was not queued."
            return
        }
        pendingModeRequestId = requestId
        statusText.text = "Waiting for host to acknowledge $mode mode…"
    }

    private fun row(parent: LinearLayout) = LinearLayout(this).apply {
        gravity = Gravity.CENTER_VERTICAL
        orientation = LinearLayout.HORIZONTAL
        parent.addView(
            this,
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
                    "USB connections require ADB reverse.",
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

    private fun connectToHost(host: String, port: Int, fingerprint: String, token: String) {
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
                    if (connected) {
                        inputView.setInputReady(true)
                        window.addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                        connectionButton.text = "Disconnect"
                        clearPcButton.isEnabled = pendingClearRequestId == null
                    } else {
                        inputView.setInputReady(true)
                        inputView.drawMode = true
                        modeButton.text = "Draw: ON"
                        pendingModeRequestId = null
                        pendingClearRequestId = null
                        clearPcButton.isEnabled = true
                        window.clearFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                    }
                    if (!connected && !message.startsWith("Connecting")) {
                        sessionClient = null
                        connectionButton.text = "Connect"
                    }
                }
            },
            onModeAck = { mode, requestId ->
                if (generation == sessionGeneration && requestId == pendingModeRequestId) {
                    pendingModeRequestId = null
                    inputView.drawMode = mode == "draw"
                    inputView.setInputReady(true)
                    modeButton.text = if (inputView.drawMode) "Draw: ON" else "Pointer: ON"
                    statusText.text = "Host acknowledged ${mode} mode"
                }
            },
            onClearAck = { requestId ->
                if (generation == sessionGeneration && requestId == pendingClearRequestId) {
                    pendingClearRequestId = null
                    inputView.clear()
                    inputView.setInputReady(true)
                    clearPcButton.isEnabled = true
                    statusText.text = "Tablet and PC annotations cleared."
                }
            },
        )
        sessionClient = client
        connectionButton.text = "Connecting…"
        client.connect()
    }

    private fun confirmRemoteClear() {
        android.app.AlertDialog.Builder(this)
            .setMessage("Clear all annotations on the tablet and connected PC?")
            .setNegativeButton("Cancel", null)
            .setPositiveButton("Clear") { _, _ ->
                val client = sessionClient
                if (client == null) {
                    inputView.clear()
                    statusText.text = "Tablet cleared; no PC session is connected."
                    return@setPositiveButton
                }
                inputView.setInputReady(false)
                val requestId = client.clearDisplay()
                if (requestId == null) {
                    inputView.setInputReady(true)
                    statusText.text = "Clear was not queued; annotations were not cleared."
                } else {
                    pendingClearRequestId = requestId
                    clearPcButton.isEnabled = false
                    statusText.text = "Waiting for the PC to acknowledge Clear…"
                    inputView.postDelayed({
                        if (pendingClearRequestId == requestId && sessionClient === client) {
                            pendingClearRequestId = null
                            inputView.setInputReady(true)
                            clearPcButton.isEnabled = true
                            statusText.text = "Clear was not acknowledged; PC state is unknown."
                        }
                    }, 5_000)
                }
            }
            .show()
    }

    private fun disconnectSession() {
        sessionGeneration++
        inputViewOrNullCancel()
        sessionClient?.disconnect()
        sessionClient = null
        pendingModeRequestId = null
        pendingClearRequestId = null
        if (::clearPcButton.isInitialized) clearPcButton.isEnabled = true
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
