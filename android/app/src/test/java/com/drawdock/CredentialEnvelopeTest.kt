package com.drawdock

import org.junit.Assert.*
import org.junit.Test
import javax.crypto.KeyGenerator
import javax.crypto.AEADBadTagException

class CredentialEnvelopeTest {
    @Test fun encryptedCredentialRoundTripsWithoutReusingAnIv() {
        val key = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()
        val record = "test reconnect credential".toByteArray()
        val first = CredentialEnvelope.seal(record, key)
        val second = CredentialEnvelope.seal(record, key)
        assertArrayEquals(record, CredentialEnvelope.open(first, key))
        assertFalse(first.contentEquals(second))
    }

    @Test fun tamperedCredentialOrUnknownVersionIsRejected() {
        val key = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()
        val encrypted = CredentialEnvelope.seal("test credential".toByteArray(), key)
        encrypted[encrypted.lastIndex] = (encrypted.last().toInt() xor 1).toByte()
        assertThrows(AEADBadTagException::class.java) { CredentialEnvelope.open(encrypted, key) }
        encrypted[0] = 2
        assertThrows(IllegalArgumentException::class.java) { CredentialEnvelope.open(encrypted, key) }
    }
}
