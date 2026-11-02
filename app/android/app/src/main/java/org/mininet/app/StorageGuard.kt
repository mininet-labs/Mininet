package org.mininet.app

import android.os.StatFs
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay

/**
 * Free-space awareness for the Android shell, mirroring `mini_durable::space`
 * (same thresholds: warn below 256 MiB, critical below 8 MiB). The shell
 * persists the identity state file, so a plain-language warning before writes
 * start failing keeps a full phone from surprising the user. This is a
 * warning only: the Android shell has no content cache, so there is nothing
 * to evict here; the desktop core owns cache retention.
 */
object StorageGuard {
    const val RESERVE_BYTES: Long = 8L * 1024 * 1024
    const val LOW_BYTES: Long = 256L * 1024 * 1024

    enum class Level { OK, LOW, CRITICAL }

    fun level(availableBytes: Long?): Level = when {
        availableBytes == null -> Level.OK
        availableBytes < RESERVE_BYTES -> Level.CRITICAL
        availableBytes < LOW_BYTES -> Level.LOW
        else -> Level.OK
    }

    fun availableBytes(dir: java.io.File): Long? =
        runCatching { StatFs(dir.path).availableBytes }.getOrNull()

    /** Plain-language text, or null when there is nothing to say. */
    fun warning(level: Level, availableBytes: Long?): String? {
        val free = availableBytes?.let { " (${it / (1024 * 1024)} MB free)" } ?: ""
        return when (level) {
            Level.OK -> null
            Level.LOW ->
                "This phone is running low on storage$free. Remove unused apps, photos or " +
                    "downloads to keep Mininet working smoothly."
            Level.CRITICAL ->
                "This phone is almost out of storage$free. Free some space now so Mininet " +
                    "can keep saving your identity safely; nothing has been lost."
        }
    }
}

/** A thin banner shown above the screens when storage is low; checks once a minute. */
@Composable
fun StorageBanner() {
    val context = LocalContext.current
    var text by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(Unit) {
        while (true) {
            val free = StorageGuard.availableBytes(context.filesDir)
            text = StorageGuard.warning(StorageGuard.level(free), free)
            delay(60_000)
        }
    }
    text?.let {
        Text(
            it,
            modifier = Modifier
                .fillMaxWidth()
                .background(Color(0xFFF4E6B8))
                .padding(horizontal = 16.dp, vertical = 8.dp),
            fontSize = 13.sp,
            color = Color(0xFF3B3200),
        )
    }
}
