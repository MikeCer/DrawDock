package com.drawdock

import android.content.ContentResolver
import android.graphics.Bitmap
import android.graphics.ImageDecoder
import android.net.Uri
import android.util.Base64
import java.io.ByteArrayOutputStream
import kotlin.math.max

object ClipboardImage {
    data class Payload(val base64: String, val width: Int, val height: Int)

    fun read(resolver: ContentResolver, uri: Uri): Payload {
        require(uri.scheme == ContentResolver.SCHEME_CONTENT) {
            "Copy an image to the tablet clipboard, not a web link or text."
        }
        var bitmap = ImageDecoder.decodeBitmap(ImageDecoder.createSource(resolver, uri)) { decoder, info, _ ->
            val w = info.size.width
            val h = info.size.height
            require(w > 0 && h > 0 && w.toLong() * h <= 32_000_000) {
                "Clipboard image is too large; copy an image of at most 32 megapixels."
            }
            val scale = minOf(1f, 1280f / max(w, h))
            decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
            decoder.setTargetSize(max(1, (w * scale).toInt()), max(1, (h * scale).toInt()))
        }
        try {
            while (true) {
                val bytes = ByteArrayOutputStream().use { output ->
                    check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, output)) { "Cannot encode clipboard image." }
                    output.toByteArray()
                }
                if (bytes.size <= 320 * 1024) {
                    return Payload(Base64.encodeToString(bytes, Base64.NO_WRAP), bitmap.width, bitmap.height)
                }
                check(max(bitmap.width, bitmap.height) > 64) { "Image cannot fit the host's size limit." }
                val smaller = Bitmap.createScaledBitmap(bitmap, max(1, bitmap.width * 3 / 4),
                    max(1, bitmap.height * 3 / 4), true)
                bitmap.recycle()
                bitmap = smaller
            }
        } finally {
            bitmap.recycle()
        }
    }
}
