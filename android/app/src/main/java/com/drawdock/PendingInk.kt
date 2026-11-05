package com.drawdock

internal class PendingInk {
    private data class Entry(
        val epoch: Long, val mark: InkMark, val bytes: Long, val submittedAt: Long,
        var revision: Long? = null,
    )
    private val entries = linkedMapOf<Long, Entry>()
    private var bytes = 0L
    private var displayed: VideoFrameState? = null

    val oldestSubmission get() = entries.values.firstOrNull()?.submittedAt
    val marks get() = entries.values.map { it.mark }
    val isEmpty get() = entries.isEmpty()

    private fun cost(mark: InkMark) = 512L + mark.points.size * 64L +
        (mark.text?.length ?: 0) * 2L + (mark.imageData?.length ?: 0) * 2L

    fun canAdd(mark: InkMark) = entries.size < 128 && bytes + cost(mark) <= 8 * 1024 * 1024

    fun add(id: Long, epoch: Long, mark: InkMark, now: Long) {
        check(id !in entries && canAdd(mark)) { "Pending ink budget exceeded" }
        val entry = Entry(epoch, mark, cost(mark), now)
        entries[id] = entry
        bytes += entry.bytes
    }

    fun acknowledge(id: Long, epoch: Long, revision: Long) {
        val entry = entries[id] ?: return
        require(epoch == entry.epoch && revision >= 0) { "Invalid ink acknowledgement" }
        entry.revision = revision
        retireDisplayed()
    }

    fun reject(id: Long) {
        entries.remove(id)?.let { bytes -= it.bytes }
    }

    fun frameLatched(frame: VideoFrameState) {
        if (displayed?.let { frame.id <= it.id } == true) return
        displayed = frame
        retireDisplayed()
    }

    private fun retireDisplayed() {
        val frame = displayed ?: return
        val revision = frame.revision ?: return
        val retired = entries.filterValues { it.revision?.let { ack ->
            frame.epoch >= it.epoch && revision >= ack
        } == true }.keys
        retired.forEach(::reject)
    }

    fun clear() {
        entries.clear()
        bytes = 0
        displayed = null
    }
}
