package com.drawdock

import java.util.concurrent.LinkedBlockingDeque
import java.util.concurrent.TimeUnit

internal class SessionOutbox {
    private val commands = LinkedBlockingDeque<SessionCommand>(129)
    private var lastFrameNanos: Long? = null

    fun offer(command: SessionCommand): Boolean = synchronized(commands) {
        val tail = commands.peekLast()
        if (tail is SessionCommand.Pointer && command is SessionCommand.Pointer &&
            tail.generation == command.generation && tail.sourceEpoch == command.sourceEpoch &&
            tail.isMotion && command.isMotion
        ) {
            commands.pollLast()
        }
        if (tail is SessionCommand.Laser && command is SessionCommand.Laser &&
            tail.sourceEpoch == command.sourceEpoch && tail.active && command.active
        ) {
            commands.pollLast()
        }
        val isRelease = command is SessionCommand.Pointer &&
            command.action in listOf("up", "cancel") || command is SessionCommand.Laser && !command.active
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
    val sourceEpoch: Long
    data class Commit(
        val mark: InkMark,
        val viewportWidth: Int,
        val viewportHeight: Int,
        val reservedBytes: Long,
        override val sourceEpoch: Long = 0,
    ) : SessionCommand

    data class Control(val type: String, override val sourceEpoch: Long = 0) : SessionCommand
    data class Mode(val mode: String, val requestId: Long, override val sourceEpoch: Long = 0) : SessionCommand
    data class ClearDisplay(val requestId: Long, override val sourceEpoch: Long = 0) : SessionCommand
    data class SelectSource(val source: String, val requestId: Long, override val sourceEpoch: Long) : SessionCommand
    data class Editor(val json: String, override val sourceEpoch: Long) : SessionCommand
    data class Laser(
        val active: Boolean, val x: Float, val y: Float, val color: Int,
        override val sourceEpoch: Long,
    ) : SessionCommand
    data class Pointer(
        val action: String,
        val generation: Long,
        val x: Float,
        val y: Float,
        val button: String,
        val viewportWidth: Int,
        val viewportHeight: Int,
        override val sourceEpoch: Long = 0,
    ) : SessionCommand {
        val isMotion: Boolean get() = action == "move" || action == "hover"
    }
}
