package org.mininet.app

import android.app.KeyguardManager
import android.content.Context
import android.os.Bundle
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.foundation.Image
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import java.io.File
import java.net.Inet4Address
import java.net.NetworkInterface
import java.nio.file.AtomicMoveNotSupportedException
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.mininet.core.AppAction
import org.mininet.core.AppCommand
import org.mininet.core.AppEventKind
import org.mininet.core.AppSnapshot
import org.mininet.core.OnboardingStage
import org.mininet.core.PlatformCapabilities
import org.mininet.core.PairingContact
import org.mininet.core.PairingOfferView
import org.mininet.core.RootCore
import org.mininet.core.SecurityReadiness
import org.mininet.core.apiVersion
import org.mininet.core.dispatch
import org.mininet.core.start

private val Ink = Color(0xFF17211B)
private val Moss = Color(0xFF315D45)
private val Mint = Color(0xFFDDF1E5)
private val Warm = Color(0xFFF7F7F2)
private val Amber = Color(0xFFF4E6B8)

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent { MininetTheme { MininetApp() } }
    }
}

sealed interface CoreUiState {
    data class Ready(val snapshot: AppSnapshot, val notice: AppEventKind? = null) : CoreUiState
    data class Home(
        val rootDid: String,
        val deviceDid: String,
        val contacts: List<PairingContact>,
        val offer: PairingOfferView? = null,
        val pairingBusy: Boolean = false,
        val message: String? = null,
    ) : CoreUiState
    data class Unavailable(val reason: String) : CoreUiState

    /**
     * A previously persisted identity exists on this device (D-0338) but
     * could not be decrypted/decoded. Deliberately distinct from
     * [Unavailable] -- that state's own screen copy is specifically about
     * the native library failing to load, which would be actively
     * misleading here. Never silently falls back to minting a fresh root
     * in its place: that would let the user end up controlling a
     * different identity than the one their existing KEL history refers
     * to, without being told.
     */
    data class RestoreFailed(val reason: String) : CoreUiState
}

class MiniViewModel(application: android.app.Application) : AndroidViewModel(application) {
    private val capabilities = readPlatformCapabilities(application)
    private val storageCipher = AndroidKeystoreCipher()
    private val stateFile = File(application.filesDir, STATE_FILE_NAME)

    // Real root/device identity (D-0335's RootCore). D-0338 closes the
    // "in-process only" gap: on construction we try to restore a
    // previously persisted blob (AES-GCM under an Android Keystore key,
    // see AndroidKeystoreCipher) rather than always starting fresh.
    private val rootCoreResult: Result<RootCore> = runCatching {
        if (stateFile.exists()) {
            RootCore.restore(stateFile.readBytes().toUByteList(), storageCipher)
        } else {
            RootCore()
        }
    }
    private val rootCore: RootCore = rootCoreResult.getOrElse { RootCore() }

    var state: CoreUiState by mutableStateOf(
        rootCoreResult.fold(
            onSuccess = {
                if (rootCore.hasRoot()) {
                    homeState()
                } else {
                    loadInitialState(capabilities)
                }
            },
            onFailure = {
                CoreUiState.RestoreFailed(it.message ?: it::class.java.simpleName)
            },
        ),
    )
        private set

    private var requestSequence = 0UL

    fun send(action: AppAction) {
        val ready = state as? CoreUiState.Ready ?: return
        val command = AppCommand(
            apiVersion = apiVersion(),
            requestId = "android-${requestSequence++}",
            capabilities = capabilities,
            action = action,
        )
        state = runCatching {
            val outcome = dispatch(ready.snapshot, command)
            CoreUiState.Ready(outcome.snapshot, outcome.events.lastOrNull()?.kind)
        }.getOrElse { CoreUiState.Unavailable(it.message ?: it::class.java.simpleName) }
    }

    // The reducer deliberately never creates identity itself (`dispatch`
    // always answers `RootCreationPending` at this stage) -- root/device
    // creation is RootCore's own boundary, called directly here rather
    // than routed through the command/event reducer.
    fun createRoot() {
        if (state !is CoreUiState.Ready) return
        state = runCatching {
            val rootDid = rootCore.createRoot()
            val deviceDid = rootCore.createDevice()
            persistRootState()
            CoreUiState.Home(
                rootDid = rootDid,
                deviceDid = deviceDid,
                contacts = emptyList(),
            )
        }.getOrElse { CoreUiState.Unavailable(it.message ?: it::class.java.simpleName) }
    }

    fun beginPairing(displayName: String) {
        if (state !is CoreUiState.Home) return
        updateHome(pairingBusy = true, message = "Preparing a signed pairing offer…")
        viewModelScope.launch(Dispatchers.IO) {
            runCatching {
                val offer = rootCore.beginPairingOffer(
                    displayName = displayName,
                    advertisedIp = findLanAddress(),
                    nowMs = System.currentTimeMillis().toULong(),
                    ttlMs = PAIRING_TTL_MS,
                )
                withContext(Dispatchers.Main) {
                    updateHome(
                        offer = offer,
                        pairingBusy = true,
                        message = "Keep this screen open while your friend scans.",
                    )
                }
                val contact = rootCore.finishPairingOffer(
                    nowMs = System.currentTimeMillis().toULong(),
                    acceptTimeoutMs = PAIRING_TIMEOUT_MS,
                    readTimeoutMs = PAIRING_TIMEOUT_MS,
                )
                persistRootState()
                withContext(Dispatchers.Main) {
                    state = homeState("Following ${contact.displayName}. Pairing verified.")
                }
            }.onFailure { failure ->
                withContext(Dispatchers.Main) {
                    state = homeState(failure.userFacingPairingMessage())
                }
            }
        }
    }

    fun acceptPairingQr(qrPayload: String, displayName: String) {
        if (state !is CoreUiState.Home) return
        updateHome(pairingBusy = true, message = "Verifying signed offer…")
        viewModelScope.launch(Dispatchers.IO) {
            runCatching {
                val contact = rootCore.acceptPairingOffer(
                    qrPayload = qrPayload,
                    displayName = displayName,
                    nowMs = System.currentTimeMillis().toULong(),
                    connectTimeoutMs = PAIRING_TIMEOUT_MS,
                )
                persistRootState()
                withContext(Dispatchers.Main) {
                    state = homeState("Following ${contact.displayName}. Pairing verified.")
                }
            }.onFailure { failure ->
                withContext(Dispatchers.Main) {
                    state = homeState(failure.userFacingPairingMessage())
                }
            }
        }
    }

    fun reportQrFailure(reason: String) {
        updateHome(pairingBusy = false, message = reason)
    }

    private fun homeState(message: String? = null): CoreUiState.Home {
        val rootDid = requireNotNull(rootCore.rootDid())
        val deviceDid = rootCore.delegatedDevices().firstOrNull()
            ?: error("restored root has no delegated device")
        return CoreUiState.Home(
            rootDid = rootDid,
            deviceDid = deviceDid,
            contacts = rootCore.pairingContacts(),
            message = message,
        )
    }

    private fun updateHome(
        offer: PairingOfferView? = (state as? CoreUiState.Home)?.offer,
        pairingBusy: Boolean = (state as? CoreUiState.Home)?.pairingBusy ?: false,
        message: String? = (state as? CoreUiState.Home)?.message,
    ) {
        val home = state as? CoreUiState.Home ?: return
        state = home.copy(offer = offer, pairingBusy = pairingBusy, message = message)
    }

    // Identity, replay memory, and signed follow state must become durable
    // together. Write ciphertext to a sibling first, then atomically replace
    // the prior snapshot where the filesystem supports it. A failure is
    // surfaced by the caller; silently continuing would forget consumed QR
    // offers after process death.
    private fun persistRootState() {
        val blob = rootCore.persistState(storageCipher)
        val temporary = File(stateFile.parentFile, "$STATE_FILE_NAME.tmp")
        temporary.writeBytes(blob.toByteArray())
        try {
            Files.move(
                temporary.toPath(),
                stateFile.toPath(),
                StandardCopyOption.ATOMIC_MOVE,
                StandardCopyOption.REPLACE_EXISTING,
            )
        } catch (_: AtomicMoveNotSupportedException) {
            Files.move(
                temporary.toPath(),
                stateFile.toPath(),
                StandardCopyOption.REPLACE_EXISTING,
            )
        }
    }

    companion object {
        private const val STATE_FILE_NAME = "root_state.bin"
        private const val PAIRING_TTL_MS = 60_000UL
        private const val PAIRING_TIMEOUT_MS = 30_000UL

        private fun readPlatformCapabilities(context: Context): PlatformCapabilities {
            val keyguard = context.getSystemService(KeyguardManager::class.java)
            return PlatformCapabilities(
                secureKeyStorage = true,
                // Hardware attestation is not wired yet, so never infer it.
                hardwareBackedKeys = false,
                screenLock = keyguard?.isDeviceSecure == true,
                biometricUnlock = false,
            )
        }

        private fun loadInitialState(capabilities: PlatformCapabilities): CoreUiState = runCatching {
            CoreUiState.Ready(
                start(capabilities),
            )
        }.getOrElse { CoreUiState.Unavailable(it.message ?: it::class.java.simpleName) }
    }
}

private fun findLanAddress(): String =
    NetworkInterface.getNetworkInterfaces()
        .toList()
        .asSequence()
        .filter { it.isUp && !it.isLoopback && !it.isVirtual }
        .flatMap { it.inetAddresses.toList().asSequence() }
        .filterIsInstance<Inet4Address>()
        .firstOrNull { !it.isLoopbackAddress && !it.isMulticastAddress && it.isSiteLocalAddress }
        ?.hostAddress
        ?: error("Connect both devices to the same local network before pairing.")

private fun Throwable.userFacingPairingMessage(): String {
    val detail = message ?: this::class.java.simpleName
    return when {
        detail.contains("expired", ignoreCase = true) -> "That QR offer expired. Ask your friend to create a new one."
        detail.contains("already used", ignoreCase = true) ||
            detail.contains("replay", ignoreCase = true) -> "That QR offer was already used and cannot be replayed."
        detail.contains("LAN", ignoreCase = true) ||
            detail.contains("network", ignoreCase = true) -> "Pairing could not reach the other phone. Keep both phones on the same LAN and try again."
        else -> "Pairing was rejected safely: $detail"
    }
}

@Composable
private fun MininetTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = MaterialTheme.colorScheme.copy(
            primary = Moss,
            onPrimary = Color.White,
            background = Warm,
            onBackground = Ink,
            surface = Color.White,
            onSurface = Ink,
        ),
        content = content,
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun MininetApp(model: MiniViewModel = viewModel()) {
    Scaffold(
        containerColor = Warm,
        topBar = {
            TopAppBar(
                title = {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Box(
                            Modifier
                                .size(12.dp)
                                .background(Moss, CircleShape),
                        )
                        Spacer(Modifier.width(10.dp))
                        Text("MININET", fontWeight = FontWeight.Black, letterSpacing = 1.6.sp)
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = Warm),
            )
        },
    ) { padding ->
        AnimatedContent(
            targetState = model.state,
            label = "core-state",
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
        ) { state ->
            when (state) {
                is CoreUiState.Ready -> OnboardingScreen(
                    state = state,
                    onContinue = { model.send(AppAction.CONTINUE) },
                    onBack = { model.send(AppAction.BACK) },
                    onCreateRoot = { model.createRoot() },
                )
                is CoreUiState.Home -> HomeScreen(
                    state = state,
                    onCreateOffer = model::beginPairing,
                    onAcceptQr = model::acceptPairingQr,
                    onQrFailure = model::reportQrFailure,
                )
                is CoreUiState.Unavailable -> CoreUnavailable(state.reason)
                is CoreUiState.RestoreFailed -> RestoreFailedScreen(state.reason)
            }
        }
    }
}

@Composable
private fun OnboardingScreen(
    state: CoreUiState.Ready,
    onContinue: () -> Unit,
    onBack: () -> Unit,
    onCreateRoot: () -> Unit,
) {
    val snapshot = state.snapshot
    val copy = stageCopy(snapshot.onboardingStage)
    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 24.dp, vertical = 18.dp),
        verticalArrangement = Arrangement.spacedBy(18.dp),
    ) {
        StatusPill(snapshot.securityReadiness)
        Text(copy.eyebrow, color = Moss, fontWeight = FontWeight.Bold)
        Text(
            copy.title,
            style = MaterialTheme.typography.displaySmall,
            fontWeight = FontWeight.Black,
            lineHeight = 42.sp,
        )
        Text(
            copy.body,
            style = MaterialTheme.typography.bodyLarge,
            color = Ink.copy(alpha = 0.78f),
            lineHeight = 26.sp,
        )

        SecurityCard(snapshot)

        if (state.notice == AppEventKind.SECURE_STORAGE_REQUIRED) {
            NoticeCard("Secure key storage is required before Mininet can create an identity.")
        }

        Spacer(Modifier.height(8.dp))
        Button(
            onClick = {
                if (snapshot.onboardingStage == OnboardingStage.ROOT_CREATION_READY) {
                    onCreateRoot()
                } else {
                    onContinue()
                }
            },
            modifier = Modifier
                .fillMaxWidth()
                .height(56.dp),
            shape = RoundedCornerShape(18.dp),
            colors = ButtonDefaults.buttonColors(containerColor = Moss),
        ) {
            Text(copy.action, fontWeight = FontWeight.Bold)
        }
        if (snapshot.onboardingStage != OnboardingStage.WELCOME) {
            OutlinedButton(
                onClick = onBack,
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(18.dp),
            ) {
                Text("Back")
            }
        }
        Text(
            "Core API ${snapshot.apiVersion} · state ${snapshot.generation}",
            modifier = Modifier.fillMaxWidth(),
            textAlign = TextAlign.Center,
            style = MaterialTheme.typography.labelSmall,
            color = Ink.copy(alpha = 0.45f),
        )
    }
}

@Composable
private fun HomeScreen(
    state: CoreUiState.Home,
    onCreateOffer: (String) -> Unit,
    onAcceptQr: (String, String) -> Unit,
    onQrFailure: (String) -> Unit,
) {
    var displayName by androidx.compose.runtime.remember { mutableStateOf("") }
    val camera = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.TakePicturePreview(),
    ) { bitmap ->
        if (bitmap == null) {
            onQrFailure("QR capture was cancelled.")
        } else {
            runCatching { QrCodec.decode(bitmap) }
                .onSuccess { payload -> onAcceptQr(payload, displayName.trim()) }
                .onFailure { onQrFailure("No readable Mininet QR code was found. Try again in brighter light.") }
        }
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 24.dp, vertical = 18.dp),
        verticalArrangement = Arrangement.spacedBy(18.dp),
    ) {
        Text(
            "Your Mininet",
            style = MaterialTheme.typography.displaySmall,
            fontWeight = FontWeight.Black,
            lineHeight = 42.sp,
        )
        Text(
            "Your identity is restored from encrypted local storage. Pair directly with someone on the same LAN—no account directory or tracking service is involved.",
            style = MaterialTheme.typography.bodyLarge,
            color = Ink.copy(alpha = 0.78f),
            lineHeight = 26.sp,
        )
        IdentityCard("Root", state.rootDid)
        IdentityCard("This device", state.deviceDid)

        OutlinedTextField(
            value = displayName,
            onValueChange = { if (it.length <= 128) displayName = it },
            label = { Text("Public display name for this pairing") },
            supportingText = { Text("Shared only with the person who scans or accepts this offer.") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
            enabled = !state.pairingBusy,
        )

        if (state.offer != null) {
            val bitmap = androidx.compose.runtime.remember(state.offer.qrPayload) {
                QrCodec.encode(state.offer.qrPayload)
            }
            Card(
                colors = CardDefaults.cardColors(containerColor = Color.White),
                shape = RoundedCornerShape(24.dp),
            ) {
                Column(
                    modifier = Modifier.padding(20.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text("Scan to follow each other", fontWeight = FontWeight.ExtraBold)
                    Image(
                        bitmap = bitmap.asImageBitmap(),
                        contentDescription = "Signed Mininet pairing QR code",
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(8.dp),
                    )
                    Text(
                        "Expires soon · listening on LAN port ${state.offer.listenPort}",
                        style = MaterialTheme.typography.labelMedium,
                        color = Ink.copy(alpha = 0.62f),
                    )
                }
            }
        } else {
            Button(
                onClick = { onCreateOffer(displayName.trim()) },
                enabled = displayName.isNotBlank() && !state.pairingBusy,
                modifier = Modifier
                    .fillMaxWidth()
                    .height(56.dp),
                shape = RoundedCornerShape(18.dp),
                colors = ButtonDefaults.buttonColors(containerColor = Moss),
            ) {
                Text("Show my pairing QR", fontWeight = FontWeight.Bold)
            }
            OutlinedButton(
                onClick = { camera.launch(null) },
                enabled = displayName.isNotBlank() && !state.pairingBusy,
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(18.dp),
            ) {
                Text("Scan a friend's QR")
            }
        }

        if (state.pairingBusy) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                CircularProgressIndicator(modifier = Modifier.size(22.dp), color = Moss)
                Text(state.message ?: "Pairing…")
            }
        } else if (state.message != null) {
            NoticeCard(state.message)
        }

        Text("People you follow", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Black)
        if (state.contacts.isEmpty()) {
            Text(
                "No contacts yet. Pairing creates a verified, signed follow on both phones.",
                color = Ink.copy(alpha = 0.62f),
            )
        } else {
            state.contacts.forEach { contact ->
                ContactCard(contact)
            }
        }
    }
}

@Composable
private fun ContactCard(contact: PairingContact) {
    Card(
        colors = CardDefaults.cardColors(containerColor = Color.White),
        shape = RoundedCornerShape(20.dp),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(18.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(contact.displayName, fontWeight = FontWeight.ExtraBold)
            Text(
                contact.did,
                style = MaterialTheme.typography.bodySmall,
                color = Ink.copy(alpha = 0.62f),
            )
            Text("Following · cryptographically verified", color = Moss, fontWeight = FontWeight.Bold)
        }
    }
}

@Composable
private fun IdentityCard(label: String, did: String) {
    Card(
        colors = CardDefaults.cardColors(containerColor = Color.White),
        shape = RoundedCornerShape(24.dp),
    ) {
        Column(
            modifier = Modifier.padding(20.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(label, fontWeight = FontWeight.ExtraBold)
            Text(
                did,
                style = MaterialTheme.typography.bodySmall,
                color = Ink.copy(alpha = 0.7f),
            )
        }
    }
}

private data class StageCopy(
    val eyebrow: String,
    val title: String,
    val body: String,
    val action: String,
)

private fun stageCopy(stage: OnboardingStage): StageCopy = when (stage) {
    OnboardingStage.WELCOME -> StageCopy(
        eyebrow = "Your network starts here",
        title = "One identity. Your devices. No account server.",
        body = "Mininet creates a self-certifying identity you control. Your phone will use its own revocable device key; the human root remains separate and recoverable.",
        action = "Understand my identity",
    )
    OnboardingStage.ROOT_SAFETY -> StageCopy(
        eyebrow = "Before any key is created",
        title = "The root is authority, not a daily login.",
        body = "Your root delegates limited capabilities to this phone. Losing a phone should mean revoking one device—not losing your identity or exposing every other device.",
        action = "Check this device",
    )
    OnboardingStage.ROOT_CREATION_READY -> StageCopy(
        eyebrow = "Device check complete",
        title = "Ready to create your identity.",
        body = "The Rust core accepted this device's security capabilities. Root and device creation run for real now, and the result is saved to this device under an Android Keystore-backed key -- closing the app no longer loses it.",
        action = "Create root",
    )
}

@Composable
private fun StatusPill(readiness: SecurityReadiness) {
    val (label, color) = when (readiness) {
        SecurityReadiness.HARDWARE_PROTECTED -> "Hardware protection reported" to Mint
        SecurityReadiness.SOFTWARE_PROTECTED -> "Secure storage available" to Mint
        SecurityReadiness.SECURE_STORAGE_UNAVAILABLE -> "Secure storage unavailable" to Amber
    }
    Surface(color = color, shape = RoundedCornerShape(100.dp)) {
        Text(
            label,
            modifier = Modifier.padding(horizontal = 14.dp, vertical = 8.dp),
            style = MaterialTheme.typography.labelLarge,
            fontWeight = FontWeight.Bold,
            color = Ink,
        )
    }
}

@Composable
private fun SecurityCard(snapshot: AppSnapshot) {
    Card(
        colors = CardDefaults.cardColors(containerColor = Color.White),
        shape = RoundedCornerShape(24.dp),
    ) {
        Column(
            modifier = Modifier.padding(20.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Text("Device readiness", fontWeight = FontWeight.ExtraBold)
            ReadinessRow("Secure application storage", snapshot.securityReadiness != SecurityReadiness.SECURE_STORAGE_UNAVAILABLE)
            ReadinessRow("Screen lock configured", snapshot.screenLock)
            ReadinessRow("Hardware key attestation", snapshot.securityReadiness == SecurityReadiness.HARDWARE_PROTECTED)
            ReadinessRow("Biometric convenience unlock", snapshot.biometricUnlock)
        }
    }
}

@Composable
private fun ReadinessRow(label: String, ready: Boolean) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Box(
            Modifier
                .size(10.dp)
                .background(if (ready) Moss else Color(0xFFB79A43), CircleShape),
        )
        Spacer(Modifier.width(12.dp))
        Text(label, modifier = Modifier.weight(1f))
        Text(if (ready) "Ready" else "Pending", fontWeight = FontWeight.Bold)
    }
}

@Composable
private fun NoticeCard(message: String) {
    Surface(color = Amber, shape = RoundedCornerShape(18.dp)) {
        Text(
            message,
            modifier = Modifier.padding(16.dp),
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.SemiBold,
        )
    }
}

@Composable
private fun CoreUnavailable(reason: String) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(28.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text("Rust core not installed", style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Black)
        Spacer(Modifier.height(12.dp))
        Text(
            "Build and copy libmini_ffi.so with app/android/scripts/build-rust. The shell stays open so setup failures are visible instead of crashing silently.",
            textAlign = TextAlign.Center,
            color = Ink.copy(alpha = 0.72f),
        )
        Spacer(Modifier.height(12.dp))
        Text(reason, style = MaterialTheme.typography.labelSmall, textAlign = TextAlign.Center)
    }
}

@Composable
private fun RestoreFailedScreen(reason: String) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(28.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            "Saved identity could not be restored",
            style = MaterialTheme.typography.headlineMedium,
            fontWeight = FontWeight.Black,
            textAlign = TextAlign.Center,
        )
        Spacer(Modifier.height(12.dp))
        Text(
            "A previously created identity exists on this device, but its encrypted state could not be read. Mininet will not silently create a different identity in its place. This can happen if the device's secure key storage was reset independently of this app's own data.",
            textAlign = TextAlign.Center,
            color = Ink.copy(alpha = 0.72f),
        )
        Spacer(Modifier.height(12.dp))
        Text(reason, style = MaterialTheme.typography.labelSmall, textAlign = TextAlign.Center)
    }
}

@Preview(showBackground = true, widthDp = 390, heightDp = 844)
@Composable
private fun WelcomePreview() {
    MininetTheme {
        OnboardingScreen(
            state = CoreUiState.Ready(
                AppSnapshot(
                    apiVersion = 0U,
                    generation = 0UL,
                    onboardingStage = OnboardingStage.WELCOME,
                    securityReadiness = SecurityReadiness.SOFTWARE_PROTECTED,
                    screenLock = true,
                    biometricUnlock = false,
                ),
            ),
            onContinue = {},
            onBack = {},
            onCreateRoot = {},
        )
    }
}
