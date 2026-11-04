package com.drawdock

import java.util.concurrent.LinkedBlockingDeque
import java.util.concurrent.TimeUnit

internal class SessionOutbox {
    private val commands = LinkedBlockingDeque<SessionCommand>(129)
    private var lastFrameNanos: Long? = null

    fun offer(command: SessionCommand): Boolean = synchronized(commands) {
        val tail = commands.peekLast()
        if (tail is SessionCommand.Pointer && command is SessionCommand.Pointer &&
            tail.generation == command.generation && tail.isMotion && command.isMotion
        ) {
            commands.pollLast()
        }
        val isRelease = command is SessionCommand.Pointer &&
            command.action in listOf("up", "cancel")
        val limit = if (isRelease) 129 else 128
        commands.size < limit && commands.offerLast(command)
    }

    fun poll(timeoutMillis: Long): SessionCommand? =
        commands.pollFirst(timeoutMillis, TimeUnit.MILLISECONDS)

    fun delayBeforeFrame(nowNanos: Long): Long =
        lastFrameNanos?.let { (FRAME_INTERVAL_NANOS - (nowNanos - it)).coerceAtLeast(0) } ?: 0

    fun frameWritten(nowNanos: Long) {
        lastFrameNanos = nowNanos
    }

    fun clear() = commands.clear()

    companion object {
        // 80 frames/s leaves headroom below the host's 120 messages/s guard.
        private const val FRAME_INTERVAL_NANOS = 12_500_000L
    }
}

internal sealed interface SessionCommand {
    data class Commit(
        val mark: InkMark,
        val viewportWidth: Int,
        val viewportHeight: Int,
        val reservedBytes: Long,
    ) : SessionCommand

    data class Control(val type: String) : SessionCommand
    data class Mode(val mode: String, val requestId: Long) : SessionCommand
    data class ClearDisplay(val requestId: Long) : SessionCommand
    data class Pointer(
        val action: String,
        val generation: Long,
        val x: Float,
        val y: Float,
        val button: String,
        val viewportWidth: Int,
        val viewportHeight: Int,
    ) : SessionCommand {
        val isMotion: Boolean get() = action == "move" || action == "hover"
    }
}
