package org.mininet.app

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import org.mininet.core.StorageCipher
import org.mininet.core.StorageCipherException

/**
 * The real [StorageCipher] backing `RootCore.persistState`/`RootCore.restore`
 * (D-0338; closes the "no key is Android Keystore-backed" honest limit
 * `mini-ffi`'s own crate doc names). AES-256-GCM under a non-exportable key
 * generated inside `AndroidKeyStore` -- the key material itself never
 * leaves the secure element/TEE, and Rust never sees it: it only ever
 * receives already-encrypted bytes across the UniFFI boundary, exactly as
 * `StorageCipher`'s own doc comment requires.
 *
 * Wire format is `iv (12 bytes) || GCM ciphertext+tag`; [decrypt] rejects
 * anything shorter than the IV alone as a real failure, not silent data
 * loss.
 *
 * Honest limits: no [KeyGenParameterSpec.Builder.setUserAuthenticationRequired]
 * gate yet (`PlatformCapabilities.biometricUnlock` stays unwired to this
 * key), and no StrongBox preference -- both are real future hardening, not
 * silently claimed here. `PlatformCapabilities.hardwareBackedKeys` in
 * [MiniViewModel] correctly still reports `false`: whether *this* key ends
 * up hardware-backed depends on the device's Keystore implementation, and
 * this class does not query or assert that.
 */
class AndroidKeystoreCipher(
    private val keyAlias: String = DEFAULT_KEY_ALIAS,
) : StorageCipher {
    private val keyStore: KeyStore = KeyStore.getInstance(ANDROID_KEYSTORE_PROVIDER).apply { load(null) }

    override fun encrypt(plaintext: List<UByte>): List<UByte> {
        try {
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.ENCRYPT_MODE, secretKey())
            val iv = cipher.iv
            val body = cipher.doFinal(plaintext.toByteArray())
            return (iv + body).toUByteList()
        } catch (e: Exception) {
            throw StorageCipherException.Failed()
        }
    }

    override fun decrypt(ciphertext: List<UByte>): List<UByte> {
        try {
            val bytes = ciphertext.toByteArray()
            if (bytes.size < IV_LENGTH_BYTES) {
                throw StorageCipherException.Failed()
            }
            val iv = bytes.copyOfRange(0, IV_LENGTH_BYTES)
            val body = bytes.copyOfRange(IV_LENGTH_BYTES, bytes.size)
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(GCM_TAG_LENGTH_BITS, iv))
            return cipher.doFinal(body).toUByteList()
        } catch (e: StorageCipherException) {
            throw e
        } catch (e: Exception) {
            throw StorageCipherException.Failed()
        }
    }

    /** Fetch this alias's existing Keystore key, generating one on first use. */
    private fun secretKey(): SecretKey {
        (keyStore.getKey(keyAlias, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEYSTORE_PROVIDER)
        val spec = KeyGenParameterSpec.Builder(
            keyAlias,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(KEY_SIZE_BITS)
            .setUserAuthenticationRequired(false)
            .build()
        generator.init(spec)
        return generator.generateKey()
    }

    companion object {
        const val DEFAULT_KEY_ALIAS = "mininet_root_storage_key_v1"
        private const val ANDROID_KEYSTORE_PROVIDER = "AndroidKeyStore"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val KEY_SIZE_BITS = 256
        private const val IV_LENGTH_BYTES = 12
        private const val GCM_TAG_LENGTH_BITS = 128
    }
}

internal fun ByteArray.toUByteList(): List<UByte> = map { it.toUByte() }

internal fun List<UByte>.toByteArray(): ByteArray = ByteArray(size) { this[it].toByte() }
