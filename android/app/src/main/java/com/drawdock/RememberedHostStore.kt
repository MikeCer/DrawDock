package com.drawdock

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import org.json.JSONObject
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

internal data class RememberedHost(
    val host: String,
    val port: Int,
    val fingerprint: String,
    val deviceId: String,
    val token: String,
) {
    fun validate() {
        PairingInvitation.parse(
            "drawdock://pair?v=1&host=$host&port=$port&fingerprint=$fingerprint&token=${"0".repeat(32)}",
        )
        require(deviceId.matches(Regex("[0-9a-f]{32}")) && token.matches(Regex("[0-9a-f]{64}"))) {
            "Invalid remembered-device credential"
        }
    }
}

internal class RememberedHostStore(context: Context) {
    private val preferences = context.applicationContext.getSharedPreferences("trusted_pc", Context.MODE_PRIVATE)
    private val alias = "drawdock.trusted-pc.v1"

    @Synchronized fun load(): RememberedHost? {
        val encrypted = preferences.getString("credential", null) ?: return null
        val bytes = CredentialEnvelope.open(Base64.decode(encrypted, Base64.NO_WRAP), key(false))
        val json = JSONObject(String(bytes, Charsets.UTF_8))
        require(json.getInt("version") == 1) { "Unsupported remembered-PC record" }
        return RememberedHost(
            json.getString("host"), json.getInt("port"), json.getString("fingerprint"),
            json.getString("device_id"), json.getString("token"),
        ).also { it.validate() }
    }

    @Synchronized fun save(host: RememberedHost) {
        host.validate()
        val json = JSONObject()
            .put("version", 1).put("host", host.host).put("port", host.port)
            .put("fingerprint", host.fingerprint).put("device_id", host.deviceId).put("token", host.token)
        val encrypted = CredentialEnvelope.seal(json.toString().toByteArray(Charsets.UTF_8), key(true))
        check(preferences.edit().putString("credential", Base64.encodeToString(encrypted, Base64.NO_WRAP)).commit()) {
            "Could not save encrypted PC credentials"
        }
    }

    @Synchronized fun forget() {
        check(preferences.edit().remove("credential").commit()) { "Could not forget the saved PC" }
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        if (store.containsAlias(alias)) store.deleteEntry(alias)
    }

    private fun key(create: Boolean): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        if (store.containsAlias(alias)) return store.getKey(alias, null) as SecretKey
        check(create) { "Remembered PC key is missing; forget this PC and pair again" }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").run {
            init(
                KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                    .setKeySize(256)
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                    .setRandomizedEncryptionRequired(true)
                    .build(),
            )
            generateKey()
        }
    }
}

internal object CredentialEnvelope {
    private val purpose = "DrawDock remembered host v1".toByteArray(Charsets.UTF_8)

    fun seal(plaintext: ByteArray, key: SecretKey): ByteArray {
        require(plaintext.size in 1..8192) { "Invalid remembered-PC record size" }
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key)
        cipher.updateAAD(purpose)
        check(cipher.iv.size == 12) { "Unexpected credential encryption IV size" }
        return byteArrayOf(1) + cipher.iv + cipher.doFinal(plaintext)
    }

    fun open(encrypted: ByteArray, key: SecretKey): ByteArray {
        require(encrypted.size in 30..8221 && encrypted[0] == 1.toByte()) {
            "Invalid encrypted PC credential"
        }
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, encrypted.copyOfRange(1, 13)))
        cipher.updateAAD(purpose)
        return cipher.doFinal(encrypted, 13, encrypted.size - 13)
    }
}
