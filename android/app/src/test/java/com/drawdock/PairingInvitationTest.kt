package com.drawdock

import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class PairingInvitationTest {
    private val fingerprint = "0123456789abcdef".repeat(4)
    private val token = "abcdef0123456789".repeat(2)
    private val invitation =
        "drawdock://pair?v=1&host=127.0.0.1&port=47831&fingerprint=$fingerprint&token=$token"

    @Test
    fun parsesPinnedUsbAndPrivateWifiInvitations() {
        for (host in listOf("127.0.0.1", "192.168.1.4", "10.0.0.2", "::1", "fd12::1")) {
            val parsed = PairingInvitation.parse(invitation.replace("127.0.0.1", host))
            assertEquals(host, parsed.host)
            assertEquals(47831, parsed.port)
            assertEquals(fingerprint, parsed.fingerprint)
            assertEquals(token, parsed.token)
        }
    }

    @Test
    fun rejectsMalformedUntrustedOrIncompleteInvitations() {
        val invalid = listOf(
            invitation.replace("drawdock://", "https://"),
            invitation.replace("v=1", "v=2"),
            invitation.replace("127.0.0.1", "8.8.8.8"),
            invitation.replace("127.0.0.1", "0.0.0.0"),
            invitation.replace("127.0.0.1", "example.com"),
            invitation.replace("127.0.0.1", "999.1.1.1"),
            invitation.replace("127.0.0.1", "2001:4860:4860::8888"),
            invitation.replace("47831", "0"),
            invitation.replace("47831", "65536"),
            invitation.replace(fingerprint, "1234"),
            invitation.replace(token, "123456"),
            invitation.substringBefore("&token="),
            "$invitation&token=$token",
            "$invitation&unknown=1",
            "$invitation#fragment",
            "x".repeat(2049),
        )
        for (value in invalid) {
            assertThrows(IllegalArgumentException::class.java) {
                PairingInvitation.parse(value)
            }
        }
    }
}
