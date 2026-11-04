package com.drawdock

import java.net.Inet4Address
import java.net.Inet6Address
import java.net.InetAddress
import java.net.URI

class PairingInvitation private constructor(
    val host: String,
    val port: Int,
    val fingerprint: String,
    val token: String,
) {
    companion object {
        fun parse(text: String): PairingInvitation {
            require(text.length <= 2048) { "QR invitation is too large" }
            val uri = try {
                URI(text)
            } catch (_: java.net.URISyntaxException) {
                throw IllegalArgumentException("Invalid QR invitation")
            }
            require(
                uri.scheme == "drawdock" && uri.rawAuthority == "pair" &&
                    uri.rawPath.isNullOrEmpty() && uri.rawFragment == null,
            ) { "This is not a DrawDock pairing invitation" }
            val fields = mutableMapOf<String, String>()
            for (field in (uri.rawQuery ?: "").split("&")) {
                val parts = field.split("=", limit = 2)
                require(parts.size == 2 && fields.put(parts[0], parts[1]) == null) {
                    "Malformed or duplicate invitation field"
                }
            }
            require(fields.keys == setOf("v", "host", "port", "fingerprint", "token")) {
                "Incomplete or unsupported invitation"
            }
            require(fields["v"] == "1") { "Unsupported invitation version" }
            val host = fields.getValue("host")
            require(isPrivateAddress(host)) { "Invitation must use a private or loopback IP" }
            val port = fields.getValue("port").toIntOrNull()
            require(port != null && port in 1..65535) { "Invalid invitation port" }
            val fingerprint = fields.getValue("fingerprint")
            require(fingerprint.matches(Regex("[0-9a-fA-F]{64}"))) {
                "Invalid certificate fingerprint"
            }
            val token = fields.getValue("token")
            require(token.matches(Regex("[0-9a-fA-F]{32}"))) { "Invalid invitation token" }
            return PairingInvitation(host, port, fingerprint, token)
        }

        private fun isPrivateAddress(host: String): Boolean {
            // Validate numeric syntax before InetAddress so scanning cannot trigger DNS.
            if (host.contains(":")) {
                if (!host.matches(Regex("[0-9a-fA-F:.]+"))) return false
            } else {
                if (!host.matches(Regex("[0-9]{1,3}(\\.[0-9]{1,3}){3}"))) return false
                if (host.split(".").any { it.toInt() !in 0..255 }) return false
            }
            val address = try {
                InetAddress.getByName(host)
            } catch (_: java.net.UnknownHostException) {
                return false
            }
            return address.isLoopbackAddress ||
                (address is Inet4Address && address.isSiteLocalAddress) ||
                (address is Inet6Address &&
                    (address.isLinkLocalAddress || address.address[0].toInt() and 0xfe == 0xfc))
        }
    }
}
