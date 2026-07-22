package org.mininet.app

import android.app.KeyguardManager
import android.content.Context
import android.os.Bundle
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
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewmodel.compose.viewModel
import org.mininet.core.AppAction
import org.mininet.core.AppCommand
import org.mininet.core.AppEventKind
import org.mininet.core.AppSnapshot
import org.mininet.core.OnboardingStage
import org.mininet.core.PlatformCapabilities
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
    data class RootCreated(val rootDid: String, val deviceDid: String) : CoreUiState
    data class Unavailable(val reason: String) : CoreUiState
}

class MiniViewModel(application: android.app.Application) : AndroidViewModel(application) {
    private val capabilities = readPlatformCapabilities(application)

    // Real root/device identity (D-0335's RootCore), created once per
    // ViewModel instance. This is intentionally in-process-only: no
    // StorageCipher/encrypted-persistence adapter exists yet (issue #198),
    // so a killed process still loses this identity -- never faked as
    // durable.
    private val rootCore = RootCore()

    var state: CoreUiState by mutableStateOf(loadInitialState(capabilities))
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
            CoreUiState.RootCreated(rootDid = rootDid, deviceDid = deviceDid)
        }.getOrElse { CoreUiState.Unavailable(it.message ?: it::class.java.simpleName) }
    }

    companion object {
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
                is CoreUiState.RootCreated -> RootCreatedScreen(state)
                is CoreUiState.Unavailable -> CoreUnavailable(state.reason)
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
private fun RootCreatedScreen(state: CoreUiState.RootCreated) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 24.dp, vertical = 18.dp),
        verticalArrangement = Arrangement.spacedBy(18.dp),
    ) {
        Text(
            "Root created",
            style = MaterialTheme.typography.displaySmall,
            fontWeight = FontWeight.Black,
            lineHeight = 42.sp,
        )
        Text(
            "This identity and its first delegated device exist only in this app's memory right now. Closing the app loses them -- encrypted on-device persistence across restarts is the next slice, not yet built.",
            style = MaterialTheme.typography.bodyLarge,
            color = Ink.copy(alpha = 0.78f),
            lineHeight = 26.sp,
        )
        IdentityCard("Root", state.rootDid)
        IdentityCard("This device", state.deviceDid)
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
        body = "The Rust core accepted this device's security capabilities. Root and device creation run for real now; encrypted on-device persistence across restarts is still the next slice, so closing the app will lose this identity until that lands.",
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
