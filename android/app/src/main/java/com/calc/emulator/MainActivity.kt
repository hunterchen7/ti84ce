package com.calc.emulator

import android.graphics.Bitmap
import android.net.Uri
import android.os.Bundle
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.layout
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.layout.size
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.ColorMatrix
import androidx.compose.ui.res.painterResource
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.compose.ui.platform.LocalLifecycleOwner
import com.calc.emulator.ui.theme.TI84EmulatorTheme
import kotlinx.coroutines.*
import java.io.InputStream
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sin
import kotlin.math.sqrt

class MainActivity : ComponentActivity() {
    companion object {
        private const val TAG = "MainActivity"
        // 48MHz / 60 FPS = 800,000 cycles per frame for real-time
        // 2x to compensate for CEmu overhead on Android
        const val CYCLES_PER_TICK = 1_600_000
        const val FRAME_INTERVAL_MS = 16L // ~60 FPS
    }

    private val emulator = EmulatorBridge()
    private lateinit var stateManager: StateManager
    private var currentRomHash: String? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // Immersive fullscreen: hide both status bar and navigation bar
        enableEdgeToEdge()
        val insetsController = WindowCompat.getInsetsController(window, window.decorView)
        insetsController.hide(WindowInsetsCompat.Type.systemBars())
        insetsController.systemBarsBehavior =
            WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE

        // Initialize state manager
        stateManager = StateManager.getInstance(applicationContext)

        // Initialize EmulatorBridge with application context
        EmulatorBridge.initialize(applicationContext)

        // Load preferred backend
        val preferredBackend = EmulatorPreferences.getEffectiveBackend(applicationContext)
        if (preferredBackend != null) {
            emulator.setBackend(preferredBackend)
        }

        if (!emulator.create()) {
            Log.e(TAG, "Failed to create emulator")
        }

        setContent {
            TI84EmulatorTheme(darkTheme = true) {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background
                ) {
                    // Check for previously saved ROM
                    val savedRomHash = EmulatorPreferences.getLastRomHash(applicationContext)

                    EmulatorScreen(
                        emulator = emulator,
                        stateManager = stateManager,
                        savedRomHash = savedRomHash,
                        onRomLoaded = { hash ->
                            currentRomHash = hash
                            EmulatorPreferences.setLastRomHash(applicationContext, hash)
                        },
                        getCurrentRomHash = { currentRomHash }
                    )
                }
            }
        }
    }

    override fun onPause() {
        super.onPause()
        // Save state when going to background
        val hash = currentRomHash
        if (hash != null) {
            Log.i(TAG, "Saving state on pause for ROM: $hash")
            if (stateManager.saveState(emulator, hash)) {
                Log.i(TAG, "State saved successfully")
            } else {
                Log.w(TAG, "Failed to save state")
            }
        } else {
            Log.d(TAG, "onPause: no ROM hash, skipping state save")
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        emulator.destroy()
    }
}

@Composable
fun EmulatorScreen(
    emulator: EmulatorBridge,
    stateManager: StateManager,
    savedRomHash: String?,
    onRomLoaded: (String) -> Unit,
    getCurrentRomHash: () -> String?
) {
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current

    // Emulator state
    var isRunning by remember { mutableStateOf(false) }
    var romLoaded by remember { mutableStateOf(false) }
    var romName by remember { mutableStateOf<String?>(null) }
    var romSize by remember { mutableIntStateOf(0) }
    var loadError by remember { mutableStateOf<String?>(null) }
    var currentRomBytes by remember { mutableStateOf<ByteArray?>(null) }
    var currentRomHash by remember { mutableStateOf<String?>(null) }

    // Try to auto-load saved ROM on first composition
    LaunchedEffect(savedRomHash) {
        if (savedRomHash != null && !romLoaded) {
            val savedRomBytes = stateManager.loadRom(savedRomHash)
            if (savedRomBytes != null) {
                val result = emulator.loadRom(savedRomBytes)
                if (result == 0) {
                    romLoaded = true
                    romName = "Saved ROM"
                    romSize = savedRomBytes.size
                    currentRomBytes = savedRomBytes
                    currentRomHash = savedRomHash
                    onRomLoaded(savedRomHash)

                    // Restore saved state or wait for ON key press
                    if (stateManager.loadState(emulator, savedRomHash)) {
                        Log.i("EmulatorScreen", "Auto-restored saved state for ROM: $savedRomHash")
                    } else {
                        Log.i("EmulatorScreen", "No saved state, waiting for ON key press")
                    }

                    isRunning = true
                    Log.i("EmulatorScreen", "Auto-loaded saved ROM: ${savedRomBytes.size} bytes")
                } else {
                    Log.e("EmulatorScreen", "Failed to auto-load ROM: $result")
                }
            } else {
                Log.i("EmulatorScreen", "No saved ROM found for hash: $savedRomHash")
            }
        }
    }

    // Pause emulation when app goes to background, resume when foregrounded
    DisposableEffect(lifecycleOwner) {
        val observer = LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_PAUSE -> {
                    Log.d("EmulatorScreen", "Lifecycle ON_PAUSE: pausing emulation")
                    isRunning = false
                }
                Lifecycle.Event.ON_RESUME -> {
                    if (romLoaded) {
                        Log.d("EmulatorScreen", "Lifecycle ON_RESUME: resuming emulation")
                        isRunning = true
                    }
                }
                else -> {}
            }
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose {
            lifecycleOwner.lifecycle.removeObserver(observer)
        }
    }

    // Backend state
    val availableBackends = remember { EmulatorBridge.getAvailableBackends() }
    var currentBackend by remember { mutableStateOf(emulator.getCurrentBackend() ?: "") }
    var showBackendDialog by remember { mutableStateOf(false) }

    // Debug info
    var totalCyclesExecuted by remember { mutableLongStateOf(0L) }
    var frameCounter by remember { mutableIntStateOf(0) }
    var showDebug by remember { mutableStateOf(false) }
    var lastKeyPress by remember { mutableStateOf("None") }
    val logLines = remember { mutableStateListOf<String>() }
    var isLcdOn by remember { mutableStateOf(true) }

    // Speed control (1x = 800K cycles, adjustable 1-10x)
    var speedMultiplier by remember { mutableStateOf(1f) } // Default 1x (real-time)

    // Display adjustment
    var calculatorScale by remember { mutableStateOf(EmulatorPreferences.getCalculatorScale(context)) }
    var calculatorYOffset by remember { mutableStateOf(EmulatorPreferences.getCalculatorYOffset(context)) }

    // Framebuffer bitmap
    val bitmap = remember {
        Bitmap.createBitmap(
            emulator.getWidth(),
            emulator.getHeight(),
            Bitmap.Config.ARGB_8888
        )
    }

    // ROM picker launcher
    val romPicker = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenDocument()
    ) { uri: Uri? ->
        uri?.let {
            try {
                val inputStream: InputStream? = context.contentResolver.openInputStream(uri)
                inputStream?.use { stream ->
                    val romBytes = stream.readBytes()
                    romSize = romBytes.size
                    currentRomBytes = romBytes

                    // Compute ROM hash for state persistence
                    val hash = stateManager.romHash(romBytes)
                    currentRomHash = hash
                    onRomLoaded(hash)

                    // Save ROM copy to our storage
                    stateManager.saveRom(romBytes, hash)

                    val result = emulator.loadRom(romBytes)
                    if (result == 0) {
                        romLoaded = true
                        romName = uri.lastPathSegment ?: "ROM"
                        loadError = null
                        totalCyclesExecuted = 0L
                        frameCounter = 0
                        logLines.clear()

                        // Try to restore saved state or wait for ON key press
                        if (stateManager.loadState(emulator, hash)) {
                            Log.i("EmulatorScreen", "Restored saved state for ROM: $hash")
                        } else {
                            Log.i("EmulatorScreen", "No saved state, waiting for ON key press")
                        }

                        isRunning = true  // Auto-start
                        Log.i("EmulatorScreen", "ROM loaded: ${romBytes.size} bytes")
                    } else {
                        loadError = "Failed to load ROM (error: $result)"
                        Log.e("EmulatorScreen", "Failed to load ROM: $result")
                    }
                }
            } catch (e: Exception) {
                loadError = "Error: ${e.message}"
                Log.e("EmulatorScreen", "Error loading ROM", e)
            }
        }
    }

    // Backend switch handler
    val onBackendSwitch: (String) -> Unit = { newBackend ->
        if (newBackend != currentBackend) {
            isRunning = false
            emulator.destroy()

            if (emulator.setBackend(newBackend)) {
                EmulatorPreferences.setPreferredBackend(context, newBackend)
                currentBackend = newBackend

                if (emulator.create()) {
                    // Reload ROM if we had one
                    currentRomBytes?.let { romBytes ->
                        val result = emulator.loadRom(romBytes)
                        if (result == 0) {
                            totalCyclesExecuted = 0L
                            frameCounter = 0
                            logLines.clear()
                            isRunning = true
                            Log.i("EmulatorScreen", "ROM reloaded after backend switch")
                        } else {
                            loadError = "Failed to reload ROM after backend switch"
                            romLoaded = false
                        }
                    }
                } else {
                    loadError = "Failed to create emulator with new backend"
                }
            } else {
                loadError = "Failed to switch backend to $newBackend"
                // Try to restore previous backend
                emulator.setBackend(currentBackend)
                emulator.create()
            }
        }
        showBackendDialog = false
    }

    // Emulation loop
    // TI-OS expression parser initialization is handled in the Rust core after boot.
    // See docs/findings.md "TI-OS Expression Parser Requires Initialization After Boot"
    // Base cycles = 800K (real-time at 48MHz/60FPS), multiplied by speed setting
    val cyclesPerTick = (800_000 * speedMultiplier).toInt()
    LaunchedEffect(isRunning, speedMultiplier) {
        if (isRunning) {
            while (isRunning) {
                val frameStart = System.nanoTime()
                val executed = withContext(Dispatchers.Default) {
                    emulator.runCycles(cyclesPerTick)
                }
                totalCyclesExecuted += executed
                frameCounter++
                // Only delay remaining time to hit target frame rate
                val elapsedMs = (System.nanoTime() - frameStart) / 1_000_000
                val remainingMs = MainActivity.FRAME_INTERVAL_MS - elapsedMs
                if (remainingMs > 0) {
                    delay(remainingMs)
                }
            }
        }
    }

    // Update framebuffer on each frame and drain logs
    LaunchedEffect(frameCounter) {
        emulator.copyFramebufferToBitmap(bitmap)
        isLcdOn = emulator.isLcdOn()
        val newLogs = emulator.drainLogs()
        if (newLogs.isNotEmpty()) {
            logLines.addAll(newLogs)
            val maxLogs = 200
            if (logLines.size > maxLogs) {
                repeat(logLines.size - maxLogs) { logLines.removeAt(0) }
            }
        }
    }

    // Backend selection dialog
    if (showBackendDialog && availableBackends.size > 1) {
        AlertDialog(
            onDismissRequest = { showBackendDialog = false },
            title = { Text("Select Emulator Backend") },
            text = {
                Column {
                    Text(
                        "Switching backends will restart the emulator. Your current state will be lost.",
                        color = Color.Gray,
                        fontSize = 12.sp,
                        modifier = Modifier.padding(bottom = 16.dp)
                    )
                    availableBackends.forEach { backend ->
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(vertical = 8.dp),
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            RadioButton(
                                selected = backend == currentBackend,
                                onClick = { onBackendSwitch(backend) }
                            )
                            Spacer(modifier = Modifier.width(8.dp))
                            Column {
                                Text(
                                    text = backend.replaceFirstChar { it.uppercase() },
                                    fontWeight = FontWeight.Medium
                                )
                                Text(
                                    text = when (backend) {
                                        "rust" -> "Custom Rust implementation"
                                        "cemu" -> "CEmu reference emulator"
                                        else -> ""
                                    },
                                    fontSize = 12.sp,
                                    color = Color.Gray
                                )
                            }
                        }
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = { showBackendDialog = false }) {
                    Text("Cancel")
                }
            },
            containerColor = Color(0xFF1A1A2E)
        )
    }

    // Show ROM loading screen if no ROM loaded, otherwise show emulator
    if (!romLoaded) {
        RomLoadingScreen(
            onLoadRom = { romPicker.launch(arrayOf("*/*")) },
            loadError = loadError,
            currentBackend = currentBackend,
            hasMultipleBackends = availableBackends.size > 1,
            onShowBackendDialog = { showBackendDialog = true }
        )
    } else {
        EmulatorView(
            emulator = emulator,
            bitmap = bitmap,
            isLcdOn = isLcdOn,
            romName = romName,
            romSize = romSize,
            isRunning = isRunning,
            onToggleRunning = { isRunning = !isRunning },
            onReset = {
                emulator.reset()
                totalCyclesExecuted = 0L
                frameCounter = 0
                logLines.clear()
            },
            onLoadNewRom = { romPicker.launch(arrayOf("*/*")) },
            frameCounter = frameCounter,
            totalCycles = totalCyclesExecuted,
            showDebug = showDebug,
            onToggleDebug = { showDebug = !showDebug },
            speedMultiplier = speedMultiplier,
            onSpeedChange = { speedMultiplier = it },
            calculatorScale = calculatorScale,
            onScaleChange = { calculatorScale = it; EmulatorPreferences.setCalculatorScale(context, it) },
            calculatorYOffset = calculatorYOffset,
            onYOffsetChange = { calculatorYOffset = it; EmulatorPreferences.setCalculatorYOffset(context, it) },
            lastKeyPress = lastKeyPress,
            logs = logLines,
            currentBackend = currentBackend,
            hasMultipleBackends = availableBackends.size > 1,
            onShowBackendDialog = { showBackendDialog = true },
            onKeyDown = { row, col ->
                lastKeyPress = "($row,$col) DOWN"
                Log.d("Keypad", "Key DOWN: row=$row col=$col")
                // For ENTER key (row 6, col 0), enable immediate trace to capture calculation
                // For other keys, arm trace for wake from HALT
                if (row == 6 && col == 0) {
                    Log.d("Keypad", "ENTER key - enabling immediate trace for calculation")
                    emulator.enableInstTrace(10000)  // Capture 10000 instructions (HALT cycles now skipped)
                } else {
                    emulator.armInstTraceOnWake(500)
                }
                emulator.setKey(row, col, true)
                frameCounter++
            },
            onKeyUp = { row, col ->
                lastKeyPress = "($row,$col) UP"
                Log.d("Keypad", "Key UP: row=$row col=$col")
                emulator.setKey(row, col, false)
                frameCounter++
            }
        )
    }
}

@Composable
fun RomLoadingScreen(
    onLoadRom: () -> Unit,
    loadError: String?,
    currentBackend: String,
    hasMultipleBackends: Boolean,
    onShowBackendDialog: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center
    ) {
        Text(
            text = "TI-84 Plus CE",
            fontSize = 28.sp,
            fontWeight = FontWeight.Bold,
            color = Color.White
        )

        Text(
            text = "Emulator",
            fontSize = 20.sp,
            color = Color.Gray,
            modifier = Modifier.padding(bottom = 48.dp)
        )

        Button(
            onClick = onLoadRom,
            modifier = Modifier
                .fillMaxWidth()
                .height(56.dp),
            colors = ButtonDefaults.buttonColors(
                containerColor = Color(0xFF4CAF50)
            )
        ) {
            Text("Import ROM", fontSize = 18.sp)
        }

        Spacer(modifier = Modifier.height(16.dp))

        Text(
            text = "Select a TI-84 Plus CE ROM file to begin",
            fontSize = 14.sp,
            color = Color.Gray
        )

        loadError?.let { error ->
            Spacer(modifier = Modifier.height(24.dp))
            Text(
                text = error,
                fontSize = 14.sp,
                color = Color(0xFFFF5722)
            )
        }

        Spacer(modifier = Modifier.height(48.dp))

        // Backend indicator and switch button
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.padding(vertical = 8.dp)
        ) {
            Text(
                text = "Backend: ",
                fontSize = 14.sp,
                color = Color.Gray
            )
            Text(
                text = currentBackend.replaceFirstChar { it.uppercase() },
                fontSize = 14.sp,
                color = Color(0xFF4FC3F7),
                fontWeight = FontWeight.Medium
            )
            if (hasMultipleBackends) {
                Spacer(modifier = Modifier.width(8.dp))
                TextButton(onClick = onShowBackendDialog) {
                    Text("Change", fontSize = 12.sp)
                }
            }
        }

        Spacer(modifier = Modifier.height(16.dp))

        Text(
            text = "You must provide your own legally obtained ROM file.",
            fontSize = 12.sp,
            color = Color.DarkGray
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun EmulatorView(
    emulator: EmulatorBridge,
    bitmap: Bitmap,
    isLcdOn: Boolean,
    romName: String?,
    romSize: Int,
    isRunning: Boolean,
    onToggleRunning: () -> Unit,
    onReset: () -> Unit,
    onLoadNewRom: () -> Unit,
    frameCounter: Int,
    totalCycles: Long,
    showDebug: Boolean,
    onToggleDebug: () -> Unit,
    speedMultiplier: Float,
    onSpeedChange: (Float) -> Unit,
    calculatorScale: Float,
    onScaleChange: (Float) -> Unit,
    calculatorYOffset: Float,
    onYOffsetChange: (Float) -> Unit,
    lastKeyPress: String,
    logs: List<String>,
    currentBackend: String,
    hasMultipleBackends: Boolean,
    onShowBackendDialog: () -> Unit,
    onKeyDown: (row: Int, col: Int) -> Unit,
    onKeyUp: (row: Int, col: Int) -> Unit
) {
    var overlayOffset by remember { mutableStateOf(Offset(6f, 6f)) }
    var overlaySize by remember { mutableStateOf(IntSize.Zero) }
    val drawerState = rememberDrawerState(initialValue = DrawerValue.Closed)
    val scope = rememberCoroutineScope()

    ModalNavigationDrawer(
        drawerState = drawerState,
        drawerContent = {
            ModalDrawerSheet(
                drawerContainerColor = Color(0xFF1A1A2E).copy(alpha = 0.7f),
                modifier = Modifier.fillMaxWidth(0.6f)
            ) {
                Spacer(modifier = Modifier.height(24.dp))
                Text(
                    text = "TI-84 Plus CE",
                    fontSize = 20.sp,
                    fontWeight = FontWeight.Bold,
                    color = Color.White,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp)
                )
                Divider(
                    modifier = Modifier.padding(vertical = 8.dp),
                    color = Color(0xFF333344)
                )

                // ROM button
                NavigationDrawerItem(
                    label = { Text("Load ROM", color = Color.White) },
                    selected = false,
                    onClick = {
                        scope.launch { drawerState.close() }
                        onLoadNewRom()
                    },
                    colors = NavigationDrawerItemDefaults.colors(
                        unselectedContainerColor = Color.Transparent
                    ),
                    modifier = Modifier.padding(horizontal = 12.dp)
                )

                // Pause/Run button
                NavigationDrawerItem(
                    label = {
                        Text(
                            if (isRunning) "Pause Emulation" else "Run Emulation",
                            color = if (isRunning) Color(0xFFFF5722) else Color(0xFF4CAF50)
                        )
                    },
                    selected = false,
                    onClick = {
                        scope.launch { drawerState.close() }
                        onToggleRunning()
                    },
                    colors = NavigationDrawerItemDefaults.colors(
                        unselectedContainerColor = Color.Transparent
                    ),
                    modifier = Modifier.padding(horizontal = 12.dp)
                )

                // Reset button
                NavigationDrawerItem(
                    label = { Text("Reset", color = Color.White) },
                    selected = false,
                    onClick = {
                        scope.launch { drawerState.close() }
                        onReset()
                    },
                    colors = NavigationDrawerItemDefaults.colors(
                        unselectedContainerColor = Color.Transparent
                    ),
                    modifier = Modifier.padding(horizontal = 12.dp)
                )

                Divider(
                    modifier = Modifier.padding(vertical = 8.dp),
                    color = Color(0xFF333344)
                )

                // Debug toggle
                NavigationDrawerItem(
                    label = {
                        Text(
                            if (showDebug) "Hide Debug Info" else "Show Debug Info",
                            color = if (showDebug) Color(0xFF9C27B0) else Color.White
                        )
                    },
                    selected = showDebug,
                    onClick = {
                        onToggleDebug()
                    },
                    colors = NavigationDrawerItemDefaults.colors(
                        selectedContainerColor = Color(0xFF2A2A3E),
                        unselectedContainerColor = Color.Transparent
                    ),
                    modifier = Modifier.padding(horizontal = 12.dp)
                )

                Divider(
                    modifier = Modifier.padding(vertical = 8.dp),
                    color = Color(0xFF333344)
                )

                // Speed control
                Text(
                    text = "Speed: ${if (speedMultiplier >= 1f) "${speedMultiplier.toInt()}x" else String.format("%.2fx", speedMultiplier)}",
                    color = Color.White,
                    fontSize = 14.sp,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)
                )
                Slider(
                    value = speedMultiplier,
                    onValueChange = { onSpeedChange(it) },
                    valueRange = 0.25f..5f,
                    steps = 18,  // (5 - 0.25) / 0.25 - 1 = 18 steps for 0.25 increments
                    modifier = Modifier.padding(horizontal = 16.dp),
                    colors = SliderDefaults.colors(
                        thumbColor = Color(0xFF4CAF50),
                        activeTrackColor = Color(0xFF4CAF50),
                        inactiveTrackColor = Color(0xFF333344)
                    )
                )

                Divider(
                    modifier = Modifier.padding(vertical = 8.dp),
                    color = Color(0xFF333344)
                )

                // Calculator scale
                Text(
                    text = "Scale: ${(calculatorScale * 100).toInt()}%",
                    color = Color.White,
                    fontSize = 14.sp,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)
                )
                Slider(
                    value = calculatorScale,
                    onValueChange = { onScaleChange(it) },
                    valueRange = 0.75f..1.25f,
                    modifier = Modifier.padding(horizontal = 16.dp),
                    colors = SliderDefaults.colors(
                        thumbColor = Color(0xFF2196F3),
                        activeTrackColor = Color(0xFF2196F3),
                        inactiveTrackColor = Color(0xFF333344)
                    )
                )

                // Calculator Y offset
                Text(
                    text = "Y Offset: ${calculatorYOffset.toInt()}",
                    color = Color.White,
                    fontSize = 14.sp,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)
                )
                Slider(
                    value = calculatorYOffset,
                    onValueChange = { onYOffsetChange(it) },
                    valueRange = -150f..150f,
                    modifier = Modifier.padding(horizontal = 16.dp),
                    colors = SliderDefaults.colors(
                        thumbColor = Color(0xFF2196F3),
                        activeTrackColor = Color(0xFF2196F3),
                        inactiveTrackColor = Color(0xFF333344)
                    )
                )

                // Backend selection (only if multiple backends available)
                if (hasMultipleBackends) {
                    Divider(
                        modifier = Modifier.padding(vertical = 8.dp),
                        color = Color(0xFF333344)
                    )

                    NavigationDrawerItem(
                        label = {
                            Column {
                                Text("Emulator Backend", color = Color.White)
                                Text(
                                    currentBackend.replaceFirstChar { it.uppercase() },
                                    color = Color(0xFF4FC3F7),
                                    fontSize = 12.sp
                                )
                            }
                        },
                        selected = false,
                        onClick = {
                            scope.launch { drawerState.close() }
                            onShowBackendDialog()
                        },
                        colors = NavigationDrawerItemDefaults.colors(
                            unselectedContainerColor = Color.Transparent
                        ),
                        modifier = Modifier.padding(horizontal = 12.dp)
                    )
                }

                Spacer(modifier = Modifier.weight(1f))

                // ROM info at bottom
                romName?.let {
                    Text(
                        text = "ROM: $it",
                        fontSize = 12.sp,
                        color = Color.Gray,
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)
                    )
                    Text(
                        text = "Size: ${romSize / 1024} KB",
                        fontSize = 12.sp,
                        color = Color.Gray,
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)
                    )
                }

                // Backend info (always shown)
                Text(
                    text = "Backend: ${currentBackend.replaceFirstChar { it.uppercase() }}",
                    fontSize = 12.sp,
                    color = Color(0xFF4FC3F7),
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)
                )

                Spacer(modifier = Modifier.height(16.dp))
            }
        }
    ) {
        var containerSize by remember { mutableStateOf(IntSize.Zero) }

        Box(
            modifier = Modifier
                .fillMaxSize()
                .onSizeChanged { containerSize = it }
        ) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .background(Color.Black),
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                // Combined calculator body (bezel + keypad as one image, includes branding)
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .weight(1f)
                        .aspectRatio(BODY_ASPECT_RATIO)
                        .graphicsLayer(
                            scaleX = calculatorScale,
                            scaleY = calculatorScale
                        )
                        .offset(y = calculatorYOffset.dp)
                ) {
                    // Background: combined calculator body photo
                    Image(
                        painter = painterResource(id = R.drawable.calculator_body),
                        contentDescription = "Calculator body",
                        modifier = Modifier.fillMaxSize(),
                        contentScale = ContentScale.FillBounds
                    )

                    // LCD positioned within combined body
                    Box(
                        modifier = Modifier
                            .fillMaxSize()
                            .layout { measurable, constraints ->
                                val parentW = constraints.maxWidth
                                val parentH = constraints.maxHeight
                                val w = (parentW * LCD_WIDTH / 100f).roundToInt()
                                val h = (parentH * LCD_HEIGHT / 100f).roundToInt()
                                val placeable = measurable.measure(
                                    constraints.copy(minWidth = w, maxWidth = w, minHeight = h, maxHeight = h)
                                )
                                layout(parentW, parentH) {
                                    val x = (parentW * LCD_LEFT / 100f).roundToInt()
                                    val y = (parentH * LCD_TOP / 100f).roundToInt()
                                    placeable.place(x, y)
                                }
                            }
                    ) {
                        if (isLcdOn) {
                            Image(
                                bitmap = bitmap.asImageBitmap(),
                                contentDescription = "Emulator screen",
                                modifier = Modifier.fillMaxSize(),
                                contentScale = ContentScale.FillBounds,
                                filterQuality = FilterQuality.None
                            )
                        }
                    }

                    // Keypad buttons overlay (positioned over keypad portion)
                    Box(
                        modifier = Modifier
                            .fillMaxSize()
                            .layout { measurable, constraints ->
                                val parentW = constraints.maxWidth
                                val parentH = constraints.maxHeight
                                val w = parentW
                                val h = (parentH * KEYPAD_HEIGHT / 100f).roundToInt()
                                val placeable = measurable.measure(
                                    constraints.copy(minWidth = w, maxWidth = w, minHeight = h, maxHeight = h)
                                )
                                layout(parentW, parentH) {
                                    val y = (parentH * KEYPAD_TOP / 100f).roundToInt()
                                    placeable.place(0, y)
                                }
                            }
                    ) {
                        ImageKeypad(
                            modifier = Modifier.fillMaxSize(),
                            onKeyDown = onKeyDown,
                            onKeyUp = onKeyUp
                        )
                    }
                }
            }

            // Floating debug overlay - can be dragged anywhere on screen
            if (showDebug) {
                val containerWidthPx = containerSize.width.toFloat()
                val containerHeightPx = containerSize.height.toFloat()

                Column(
                    modifier = Modifier
                        .offset { IntOffset(overlayOffset.x.roundToInt(), overlayOffset.y.roundToInt()) }
                        .onSizeChanged { overlaySize = it }
                        .pointerInput(containerWidthPx, containerHeightPx, overlaySize) {
                            detectDragGestures { change, dragAmount ->
                                change.consume()
                                val newX = overlayOffset.x + dragAmount.x
                                val newY = overlayOffset.y + dragAmount.y
                                val maxX = (containerWidthPx - overlaySize.width).coerceAtLeast(0f)
                                val maxY = (containerHeightPx - overlaySize.height).coerceAtLeast(0f)
                                overlayOffset = Offset(
                                    newX.coerceIn(0f, maxX),
                                    newY.coerceIn(0f, maxY)
                                )
                            }
                        }
                        .background(Color(0xCC1A1A2E), RoundedCornerShape(4.dp))
                        .padding(6.dp)
                ) {
                    Text(
                        text = "ROM: ${romName ?: "Unknown"} (${romSize / 1024}KB)",
                        fontSize = 10.sp,
                        color = Color(0xFF4FC3F7),
                        fontFamily = FontFamily.Monospace
                    )
                    Text(
                        text = "Backend: ${currentBackend.replaceFirstChar { it.uppercase() }}",
                        fontSize = 10.sp,
                        color = Color(0xFFCE93D8),
                        fontFamily = FontFamily.Monospace
                    )
                    Text(
                        text = "Frames: $frameCounter | Cycles: ${formatCycles(totalCycles)}",
                        fontSize = 10.sp,
                        color = Color(0xFF81C784),
                        fontFamily = FontFamily.Monospace
                    )
                    Text(
                        text = "Speed: ${MainActivity.CYCLES_PER_TICK / 1000}K cycles/tick @ ${1000 / MainActivity.FRAME_INTERVAL_MS} FPS",
                        fontSize = 10.sp,
                        color = Color(0xFFFFB74D),
                        fontFamily = FontFamily.Monospace
                    )
                    Text(
                        text = "Status: ${if (isRunning) "RUNNING" else "PAUSED"}",
                        fontSize = 10.sp,
                        color = if (isRunning) Color(0xFF4CAF50) else Color(0xFFFF5722),
                        fontFamily = FontFamily.Monospace
                    )
                    Text(
                        text = "Last Key: $lastKeyPress",
                        fontSize = 10.sp,
                        color = Color(0xFFE1BEE7),
                        fontFamily = FontFamily.Monospace
                    )
                    val displayLogs = logs.takeLast(6)
                    if (displayLogs.isNotEmpty()) {
                        Spacer(modifier = Modifier.height(3.dp))
                        Text(
                            text = "Logs:",
                            fontSize = 9.sp,
                            color = Color(0xFFB0BEC5),
                            fontFamily = FontFamily.Monospace
                        )
                        displayLogs.forEach { line ->
                            Text(
                                text = line,
                                fontSize = 9.sp,
                                color = Color(0xFFB0BEC5),
                                fontFamily = FontFamily.Monospace,
                                maxLines = 1
                            )
                        }
                    }
                }
            }
        }
    }
}

private fun formatCycles(cycles: Long): String {
    return when {
        cycles >= 1_000_000_000 -> String.format("%.2fG", cycles / 1_000_000_000.0)
        cycles >= 1_000_000 -> String.format("%.2fM", cycles / 1_000_000.0)
        cycles >= 1_000 -> String.format("%.1fK", cycles / 1_000.0)
        else -> cycles.toString()
    }
}

// Key definition with styling info
// ── Image-based keypad data ──

private data class ImageButtonRegion(
    val name: String,
    val row: Int,
    val col: Int,
    val left: Float,   // percentage of container width
    val top: Float,    // percentage of container height
    val width: Float,  // percentage of container width
    val height: Float, // percentage of container height
    val drawableRes: Int // R.drawable.btn_xxx
)

private const val KEYPAD_ASPECT_RATIO = 0.657338f
private const val BODY_ASPECT_RATIO = 963f / 2239f
private const val LCD_LEFT = 11.53f
private const val LCD_TOP = 6.92f
private const val LCD_WIDTH = 76.74f
private const val LCD_HEIGHT = 24.92f
private const val KEYPAD_TOP = 34.57f
private const val KEYPAD_HEIGHT = 65.43f

private data class DPadRegion(
    val left: Float,
    val top: Float,
    val width: Float,
    val height: Float,
)

private val IMAGE_DPAD_REGION = DPadRegion(
    left = 63.97f, top = 13.72f, width = 22.01f, height = 14.74f
)

@Composable
private fun imageButtonRegions(): List<ImageButtonRegion> {
    data class RegionDef(
        val name: String, val row: Int, val col: Int,
        val left: Float, val top: Float, val width: Float, val height: Float,
        val img: String
    )

    val defs = listOf(
        // Function row
        RegionDef("y=",     1, 4, 11.32f, 5.12f,  11.01f, 4.03f, "btn_y_eq"),
        RegionDef("window", 1, 3, 27.73f, 5.12f,  11.32f, 4.03f, "btn_window"),
        RegionDef("zoom",   1, 2, 44.76f, 5.12f,  10.80f, 4.03f, "btn_zoom"),
        RegionDef("trace",  1, 1, 61.37f, 5.12f,  10.70f, 4.03f, "btn_trace"),
        RegionDef("graph",  1, 0, 77.78f, 5.12f,  11.01f, 4.03f, "btn_graph"),
        // Control row 1
        RegionDef("2nd",    1, 5, 11.32f, 14.20f, 11.01f, 5.32f, "btn_2nd"),
        RegionDef("mode",   1, 6, 27.73f, 14.20f, 11.01f, 5.32f, "btn_mode"),
        RegionDef("del",    1, 7, 44.65f, 14.20f, 11.01f, 5.32f, "btn_del"),
        // Control row 2
        RegionDef("alpha",  2, 7, 11.32f, 22.73f, 11.01f, 5.32f, "btn_alpha"),
        RegionDef("xttn",   3, 7, 27.73f, 22.53f, 11.01f, 5.53f, "btn_xttn"),
        RegionDef("stat",   4, 7, 44.65f, 22.73f, 11.01f, 5.32f, "btn_stat"),
        // Math row
        RegionDef("math",   2, 6, 11.11f, 31.13f, 11.11f, 5.46f, "btn_math"),
        RegionDef("apps",   3, 6, 28.04f, 31.13f, 10.70f, 5.46f, "btn_apps"),
        RegionDef("prgm",   4, 6, 44.65f, 31.19f, 11.01f, 5.32f, "btn_prgm"),
        RegionDef("vars",   5, 6, 61.37f, 31.19f, 10.70f, 5.32f, "btn_vars"),
        RegionDef("clear",  6, 6, 77.47f, 31.19f, 11.32f, 5.46f, "btn_clear"),
        // Trig row
        RegionDef("x_inv",  2, 5, 11.01f, 39.66f, 11.32f, 5.67f, "btn_x_inv"),
        RegionDef("sin",    3, 5, 28.04f, 39.66f, 10.70f, 5.32f, "btn_sin"),
        RegionDef("cos",    4, 5, 44.65f, 39.66f, 11.01f, 5.32f, "btn_cos"),
        RegionDef("tan",    5, 5, 61.37f, 39.66f, 10.70f, 5.32f, "btn_tan"),
        RegionDef("pow",    6, 5, 77.78f, 39.66f, 11.01f, 5.46f, "btn_pow"),
        // Special row
        RegionDef("x_sq",   2, 4, 11.32f, 48.19f, 11.01f, 5.32f, "btn_x_sq"),
        RegionDef("comma",  3, 4, 28.04f, 48.19f, 10.70f, 5.32f, "btn_comma"),
        RegionDef("lparen", 4, 4, 44.65f, 48.40f, 11.01f, 5.12f, "btn_lparen"),
        RegionDef("rparen", 5, 4, 61.37f, 48.40f, 10.70f, 5.12f, "btn_rparen"),
        RegionDef("div",    6, 4, 77.67f, 48.19f, 11.11f, 5.32f, "btn_div"),
        // Number block row 1
        RegionDef("log",    2, 3, 11.32f, 56.93f, 11.01f, 5.12f, "btn_log"),
        RegionDef("7",      3, 3, 27.93f, 56.79f, 11.01f, 6.21f, "btn_7"),
        RegionDef("8",      4, 3, 44.65f, 56.66f, 11.01f, 6.21f, "btn_8"),
        RegionDef("9",      5, 3, 61.16f, 56.93f, 11.01f, 6.21f, "btn_9"),
        RegionDef("mul",    6, 3, 77.78f, 56.79f, 11.01f, 5.19f, "btn_mul"),
        // Number block row 2
        RegionDef("ln",     2, 2, 11.32f, 65.12f, 11.11f, 5.53f, "btn_ln"),
        RegionDef("4",      3, 2, 27.93f, 66.35f, 11.01f, 6.21f, "btn_4"),
        RegionDef("5",      4, 2, 44.65f, 66.35f, 11.01f, 6.21f, "btn_5"),
        RegionDef("6",      5, 2, 61.16f, 66.35f, 11.01f, 6.21f, "btn_6"),
        RegionDef("sub",    6, 2, 77.78f, 65.39f, 11.01f, 5.12f, "btn_sub"),
        // Number block row 3
        RegionDef("sto",    2, 1, 11.01f, 73.52f, 11.63f, 5.67f, "btn_sto"),
        RegionDef("1",      3, 1, 27.93f, 76.04f, 11.01f, 6.21f, "btn_1"),
        RegionDef("2",      4, 1, 44.65f, 76.04f, 11.01f, 6.21f, "btn_2"),
        RegionDef("3",      5, 1, 61.16f, 75.84f, 11.01f, 6.21f, "btn_3"),
        RegionDef("add",    6, 1, 77.78f, 73.58f, 11.01f, 5.32f, "btn_add"),
        // Number block row 4
        RegionDef("on",     2, 0, 11.11f, 81.91f, 11.11f, 5.94f, "btn_on"),
        RegionDef("0",      3, 0, 27.93f, 85.80f, 11.01f, 6.21f, "btn_0"),
        RegionDef("dot",    4, 0, 44.65f, 85.80f, 11.01f, 6.21f, "btn_dot"),
        RegionDef("neg",    5, 0, 61.16f, 85.80f, 11.01f, 6.21f, "btn_neg"),
        RegionDef("enter",  6, 0, 77.78f, 82.39f, 11.01f, 5.12f, "btn_enter"),
    )

    val context = LocalContext.current
    return remember {
        defs.map { d ->
            val resId = context.resources.getIdentifier(d.img, "drawable", context.packageName)
            ImageButtonRegion(
                name = d.name, row = d.row, col = d.col,
                left = d.left, top = d.top, width = d.width, height = d.height,
                drawableRes = resId
            )
        }
    }
}

// ── ImageKeypad composable ──

@Composable
fun ImageKeypad(
    modifier: Modifier = Modifier,
    onKeyDown: (row: Int, col: Int) -> Unit,
    onKeyUp: (row: Int, col: Int) -> Unit
) {
    val regions = imageButtonRegions()

    Box(
        modifier = modifier
    ) {
        // Button overlays (no background — parent shows combined image)
        regions.forEach { region ->
            ImageKeyButton(
                region = region,
                onDown = { onKeyDown(region.row, region.col) },
                onUp = { onKeyUp(region.row, region.col) }
            )
        }

        // D-pad overlay
        Box(
            modifier = Modifier
                .fillMaxSize()
                .layout { measurable, constraints ->
                    val parentW = constraints.maxWidth
                    val parentH = constraints.maxHeight
                    val w = (parentW * IMAGE_DPAD_REGION.width / 100f).roundToInt()
                    val h = (parentH * IMAGE_DPAD_REGION.height / 100f).roundToInt()
                    val placeable = measurable.measure(
                        constraints.copy(minWidth = w, maxWidth = w, minHeight = h, maxHeight = h)
                    )
                    layout(parentW, parentH) {
                        val x = (parentW * IMAGE_DPAD_REGION.left / 100f).roundToInt()
                        val y = (parentH * IMAGE_DPAD_REGION.top / 100f).roundToInt()
                        placeable.place(x, y)
                    }
                }
        ) {
            DPad(
                modifier = Modifier.fillMaxSize(),
                onKeyDown = onKeyDown,
                onKeyUp = onKeyUp
            )
        }
    }
}

@Composable
private fun ImageKeyButton(
    region: ImageButtonRegion,
    onDown: () -> Unit,
    onUp: () -> Unit
) {
    var isPressed by remember { mutableStateOf(false) }

    val travelPx = with(LocalDensity.current) { 2.dp.toPx() }
    val darkenMatrix = remember {
        ColorMatrix(floatArrayOf(
            0.82f, 0f, 0f, 0f, 0f,
            0f, 0.82f, 0f, 0f, 0f,
            0f, 0f, 0.82f, 0f, 0f,
            0f, 0f, 0f, 1f, 0f
        ))
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .layout { measurable, constraints ->
                val parentW = constraints.maxWidth
                val parentH = constraints.maxHeight
                val w = (parentW * region.width / 100f).roundToInt()
                val h = (parentH * region.height / 100f).roundToInt()
                val placeable = measurable.measure(
                    constraints.copy(minWidth = w, maxWidth = w, minHeight = h, maxHeight = h)
                )
                layout(parentW, parentH) {
                    val x = (parentW * region.left / 100f).roundToInt()
                    val y = (parentH * region.top / 100f).roundToInt() +
                        if (isPressed) travelPx.roundToInt() else 0
                    placeable.place(x, y)
                }
            }
            .pointerInput(Unit) {
                detectTapGestures(
                    onPress = {
                        isPressed = true
                        onDown()
                        try {
                            awaitRelease()
                        } finally {
                            isPressed = false
                            onUp()
                        }
                    }
                )
            }
    ) {
        if (region.drawableRes != 0) {
            Image(
                painter = painterResource(region.drawableRes),
                contentDescription = region.name,
                contentScale = ContentScale.FillBounds,
                colorFilter = if (isPressed) ColorFilter.colorMatrix(darkenMatrix) else null,
                modifier = Modifier.fillMaxSize()
            )
        }
    }
}

// ── Legacy keypad (kept for reference) ──

data class KeyDef(
    val label: String,
    val row: Int,
    val col: Int,
    val style: KeyStyle = KeyStyle.DARK,
    val secondLabel: String? = null,  // Blue 2nd function label
    val alphaLabel: String? = null,   // Green alpha label
    val secondLabelColor: Color? = null,
    val alphaLabelColor: Color? = null
)

enum class KeyStyle {
    DARK,       // Dark gray - most keys
    YELLOW,     // Blue - 2nd key
    GREEN,      // Green - alpha key
    WHITE,      // Light gray - number/function keys
    BLUE,       // Light gray - enter key
    ARROW       // Arrow keys
}

@Composable
fun Keypad(
    modifier: Modifier = Modifier,
    onKeyDown: (row: Int, col: Int) -> Unit,
    onKeyUp: (row: Int, col: Int) -> Unit
) {
    // TI-84 Plus CE accurate keypad layout
    // Based on actual key matrix mapping
    Column(
        modifier = modifier
            .background(Color(0xFF1B1B1B))
            .padding(horizontal = 6.dp, vertical = 4.dp)
            .padding(bottom = 14.dp),
        verticalArrangement = Arrangement.spacedBy(2.dp)
    ) {
        // Row 1: Function keys (y=, window, zoom, trace, graph)
        // CEmu matrix row 1: graph(0), trace(1), zoom(2), window(3), yequ(4), 2nd(5), mode(6), del(7)
        KeyRow(
            keys = listOf(
                KeyDef("y=", 1, 4, KeyStyle.WHITE, secondLabel = "stat plot", alphaLabel = "f1"),
                KeyDef("window", 1, 3, KeyStyle.WHITE, secondLabel = "tblset", alphaLabel = "f2"),
                KeyDef("zoom", 1, 2, KeyStyle.WHITE, secondLabel = "format", alphaLabel = "f3"),
                KeyDef("trace", 1, 1, KeyStyle.WHITE, secondLabel = "calc", alphaLabel = "f4"),
                KeyDef("graph", 1, 0, KeyStyle.WHITE, secondLabel = "table", alphaLabel = "f5")
            ),
            modifier = Modifier.weight(1f),
            onKeyDown = onKeyDown,
            onKeyUp = onKeyUp
        )

        // Rows 2-3: Keys on left, D-pad on right (2 rows only)
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .weight(2f),
            horizontalArrangement = Arrangement.spacedBy(2.dp)
        ) {
            // Left side: 3x2 grid of keys
            Column(
                modifier = Modifier.weight(3f),
                verticalArrangement = Arrangement.spacedBy(2.dp)
            ) {
                // Row 2: 2nd, mode, del
                // CEmu: 2nd(1,5), mode(1,6), del(1,7)
                Row(
                    modifier = Modifier.weight(1f).fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(2.dp)
                ) {
                    KeyButton(
                        keyDef = KeyDef("2nd", 1, 5, KeyStyle.YELLOW),
                        modifier = Modifier.weight(1f),
                        onDown = { onKeyDown(1, 5) },
                        onUp = { onKeyUp(1, 5) }
                    )
                    KeyButton(
                        keyDef = KeyDef("mode", 1, 6, secondLabel = "quit"),
                        modifier = Modifier.weight(1f),
                        onDown = { onKeyDown(1, 6) },
                        onUp = { onKeyUp(1, 6) }
                    )
                    KeyButton(
                        keyDef = KeyDef("del", 1, 7, secondLabel = "ins"),
                        modifier = Modifier.weight(1f),
                        onDown = { onKeyDown(1, 7) },
                        onUp = { onKeyUp(1, 7) }
                    )
                }
                // Row 3: alpha, x,t,θ,n, stat
                // CEmu: alpha(2,7), xton(3,7), stat(4,7)
                Row(
                    modifier = Modifier.weight(1f).fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(2.dp)
                ) {
                    KeyButton(
                        keyDef = KeyDef("alpha", 2, 7, KeyStyle.GREEN, secondLabel = "A-lock"),
                        modifier = Modifier.weight(1f),
                        onDown = { onKeyDown(2, 7) },
                        onUp = { onKeyUp(2, 7) }
                    )
                    KeyButton(
                        keyDef = KeyDef("X,T,θ,n", 3, 7, secondLabel = "link"),
                        modifier = Modifier.weight(1f),
                        onDown = { onKeyDown(3, 7) },
                        onUp = { onKeyUp(3, 7) }
                    )
                    KeyButton(
                        keyDef = KeyDef("stat", 4, 7, secondLabel = "list"),
                        modifier = Modifier.weight(1f),
                        onDown = { onKeyDown(4, 7) },
                        onUp = { onKeyUp(4, 7) }
                    )
                }
            }

            // D-Pad on the right (spans 2 rows only)
            DPad(
                modifier = Modifier
                    .weight(2f)
                    .fillMaxHeight()
                    .padding(vertical = 4.dp),
                onKeyDown = onKeyDown,
                onKeyUp = onKeyUp
            )
        }

        // Row 4: math, apps, prgm, vars, clear (separate row)
        // CEmu: math(2,6), apps(3,6), prgm(4,6), vars(5,6), clear(6,6)
        KeyRow(
            keys = listOf(
                KeyDef("math", 2, 6, secondLabel = "test", alphaLabel = "A"),
                KeyDef("apps", 3, 6, secondLabel = "angle", alphaLabel = "B"),
                KeyDef("prgm", 4, 6, secondLabel = "draw", alphaLabel = "C"),
                KeyDef("vars", 5, 6, secondLabel = "distr", alphaLabel = "D"),
                KeyDef("clear", 6, 6)
            ),
            modifier = Modifier.weight(1f),
            onKeyDown = onKeyDown,
            onKeyUp = onKeyUp
        )

        // Row 5: x⁻¹, sin, cos, tan, ^
        // CEmu: inv(2,5), sin(3,5), cos(4,5), tan(5,5), pow(6,5)
        KeyRow(
            keys = listOf(
                KeyDef("x⁻¹", 2, 5, secondLabel = "matrix"),
                KeyDef("sin", 3, 5, secondLabel = "sin⁻¹", alphaLabel = "E"),
                KeyDef("cos", 4, 5, secondLabel = "cos⁻¹", alphaLabel = "F"),
                KeyDef("tan", 5, 5, secondLabel = "tan⁻¹", alphaLabel = "G"),
                KeyDef("^", 6, 5, secondLabel = "π", alphaLabel = "H")
            ),
            modifier = Modifier.weight(1f),
            onKeyDown = onKeyDown,
            onKeyUp = onKeyUp
        )

        // Row 6: x², ,, (, ), ÷
        // CEmu: sq(2,4), comma(3,4), lpar(4,4), rpar(5,4), div(6,4)
        KeyRow(
            keys = listOf(
                KeyDef("x²", 2, 4, secondLabel = "√"),
                KeyDef(",", 3, 4, secondLabel = "EE", alphaLabel = "J"),
                KeyDef("(", 4, 4, secondLabel = "{", alphaLabel = "K"),
                KeyDef(")", 5, 4, secondLabel = "}", alphaLabel = "L"),
                KeyDef("÷", 6, 4, KeyStyle.WHITE, secondLabel = "e", alphaLabel = "M")
            ),
            modifier = Modifier.weight(1f),
            onKeyDown = onKeyDown,
            onKeyUp = onKeyUp
        )

        NumericColumns(
            modifier = Modifier.weight(4.8f),
            onKeyDown = onKeyDown,
            onKeyUp = onKeyUp
        )
    }
}

@Composable
fun NumericColumns(
    modifier: Modifier = Modifier,
    onKeyDown: (row: Int, col: Int) -> Unit,
    onKeyUp: (row: Int, col: Int) -> Unit
) {
    val keySpacing = 2.dp
    val numberKeyWeight = 1.42f
    val darkKeyWeight = 0.96f
    val enterKeyWeight = 1.22f
    val numberKeyPad = 3.dp
    val outerColumnBottomInset = 14.dp

    Row(
        modifier = modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(5.dp)
    ) {
        // Column 1: log, ln, sto→, on
        // CEmu: log(2,3), ln(2,2), sto(2,1), on(2,0)
        Column(
            modifier = Modifier
                .weight(1f)
                .padding(bottom = outerColumnBottomInset),
            verticalArrangement = Arrangement.spacedBy(keySpacing)
        ) {
            KeyButton(
                keyDef = KeyDef("log", 2, 3, secondLabel = "10ˣ", alphaLabel = "N"),
                modifier = Modifier.weight(darkKeyWeight).fillMaxWidth(),
                onDown = { onKeyDown(2, 3) },
                onUp = { onKeyUp(2, 3) }
            )
            KeyButton(
                keyDef = KeyDef("ln", 2, 2, secondLabel = "eˣ", alphaLabel = "S"),
                modifier = Modifier.weight(darkKeyWeight).fillMaxWidth(),
                onDown = { onKeyDown(2, 2) },
                onUp = { onKeyUp(2, 2) }
            )
            KeyButton(
                keyDef = KeyDef("sto→", 2, 1, secondLabel = "rcl", alphaLabel = "X"),
                modifier = Modifier.weight(darkKeyWeight).fillMaxWidth(),
                onDown = { onKeyDown(2, 1) },
                onUp = { onKeyUp(2, 1) }
            )
            KeyButton(
                keyDef = KeyDef("on", 2, 0, secondLabel = "off"),
                modifier = Modifier.weight(darkKeyWeight).fillMaxWidth(),
                onDown = { onKeyDown(2, 0) },
                onUp = { onKeyUp(2, 0) }
            )
        }

        // Column 2: 7, 4, 1, 0
        // CEmu: 7(3,3), 4(3,2), 1(3,1), 0(3,0)
        Column(
            modifier = Modifier
                .weight(1f),
            verticalArrangement = Arrangement.spacedBy(keySpacing)
        ) {
            KeyButton(
                keyDef = KeyDef("7", 3, 3, KeyStyle.WHITE, secondLabel = "u", alphaLabel = "O"),
                modifier = Modifier
                    .weight(numberKeyWeight)
                    .fillMaxWidth()
                    .padding(horizontal = numberKeyPad),
                onDown = { onKeyDown(3, 3) },
                onUp = { onKeyUp(3, 3) }
            )
            KeyButton(
                keyDef = KeyDef("4", 3, 2, KeyStyle.WHITE, secondLabel = "L4", alphaLabel = "T"),
                modifier = Modifier
                    .weight(numberKeyWeight)
                    .fillMaxWidth()
                    .padding(horizontal = numberKeyPad),
                onDown = { onKeyDown(3, 2) },
                onUp = { onKeyUp(3, 2) }
            )
            KeyButton(
                keyDef = KeyDef("1", 3, 1, KeyStyle.WHITE, secondLabel = "L1", alphaLabel = "Y"),
                modifier = Modifier
                    .weight(numberKeyWeight)
                    .fillMaxWidth()
                    .padding(horizontal = numberKeyPad),
                onDown = { onKeyDown(3, 1) },
                onUp = { onKeyUp(3, 1) }
            )
            KeyButton(
                keyDef = KeyDef("0", 3, 0, KeyStyle.WHITE, secondLabel = "catalog", alphaLabel = " "),
                modifier = Modifier
                    .weight(numberKeyWeight)
                    .fillMaxWidth()
                    .padding(horizontal = numberKeyPad),
                onDown = { onKeyDown(3, 0) },
                onUp = { onKeyUp(3, 0) }
            )
        }

        // Column 3: 8, 5, 2, .
        // CEmu: 8(4,3), 5(4,2), 2(4,1), dot(4,0)
        Column(
            modifier = Modifier
                .weight(1f),
            verticalArrangement = Arrangement.spacedBy(keySpacing)
        ) {
            KeyButton(
                keyDef = KeyDef("8", 4, 3, KeyStyle.WHITE, secondLabel = "v", alphaLabel = "P"),
                modifier = Modifier
                    .weight(numberKeyWeight)
                    .fillMaxWidth()
                    .padding(horizontal = numberKeyPad),
                onDown = { onKeyDown(4, 3) },
                onUp = { onKeyUp(4, 3) }
            )
            KeyButton(
                keyDef = KeyDef("5", 4, 2, KeyStyle.WHITE, secondLabel = "L5", alphaLabel = "U"),
                modifier = Modifier
                    .weight(numberKeyWeight)
                    .fillMaxWidth()
                    .padding(horizontal = numberKeyPad),
                onDown = { onKeyDown(4, 2) },
                onUp = { onKeyUp(4, 2) }
            )
            KeyButton(
                keyDef = KeyDef("2", 4, 1, KeyStyle.WHITE, secondLabel = "L2", alphaLabel = "Z"),
                modifier = Modifier
                    .weight(numberKeyWeight)
                    .fillMaxWidth()
                    .padding(horizontal = numberKeyPad),
                onDown = { onKeyDown(4, 1) },
                onUp = { onKeyUp(4, 1) }
            )
            KeyButton(
                keyDef = KeyDef(".", 4, 0, KeyStyle.WHITE, secondLabel = "i", alphaLabel = ":"),
                modifier = Modifier
                    .weight(numberKeyWeight)
                    .fillMaxWidth()
                    .padding(horizontal = numberKeyPad),
                onDown = { onKeyDown(4, 0) },
                onUp = { onKeyUp(4, 0) }
            )
        }

        // Column 4: 9, 6, 3, (−)
        // CEmu: 9(5,3), 6(5,2), 3(5,1), neg(5,0)
        Column(
            modifier = Modifier.weight(1f),
            verticalArrangement = Arrangement.spacedBy(keySpacing)
        ) {
            KeyButton(
                keyDef = KeyDef("9", 5, 3, KeyStyle.WHITE, secondLabel = "w", alphaLabel = "Q"),
                modifier = Modifier
                    .weight(numberKeyWeight)
                    .fillMaxWidth()
                    .padding(horizontal = numberKeyPad),
                onDown = { onKeyDown(5, 3) },
                onUp = { onKeyUp(5, 3) }
            )
            KeyButton(
                keyDef = KeyDef("6", 5, 2, KeyStyle.WHITE, secondLabel = "L6", alphaLabel = "V"),
                modifier = Modifier
                    .weight(numberKeyWeight)
                    .fillMaxWidth()
                    .padding(horizontal = numberKeyPad),
                onDown = { onKeyDown(5, 2) },
                onUp = { onKeyUp(5, 2) }
            )
            KeyButton(
                keyDef = KeyDef("3", 5, 1, KeyStyle.WHITE, secondLabel = "L3", alphaLabel = "θ"),
                modifier = Modifier
                    .weight(numberKeyWeight)
                    .fillMaxWidth()
                    .padding(horizontal = numberKeyPad),
                onDown = { onKeyDown(5, 1) },
                onUp = { onKeyUp(5, 1) }
            )
            KeyButton(
                keyDef = KeyDef("(−)", 5, 0, KeyStyle.WHITE, secondLabel = "ans", alphaLabel = "?"),
                modifier = Modifier
                    .weight(numberKeyWeight)
                    .fillMaxWidth()
                    .padding(horizontal = numberKeyPad),
                onDown = { onKeyDown(5, 0) },
                onUp = { onKeyUp(5, 0) }
            )
        }

        // Column 5: ×, −, +, enter
        // CEmu: mul(6,3), sub(6,2), add(6,1), enter(6,0)
        Column(
            modifier = Modifier
                .weight(1f)
                .padding(bottom = outerColumnBottomInset),
            verticalArrangement = Arrangement.spacedBy(keySpacing)
        ) {
            KeyButton(
                keyDef = KeyDef("×", 6, 3, KeyStyle.WHITE, secondLabel = "[", alphaLabel = "R"),
                modifier = Modifier.weight(darkKeyWeight).fillMaxWidth(),
                onDown = { onKeyDown(6, 3) },
                onUp = { onKeyUp(6, 3) }
            )
            KeyButton(
                keyDef = KeyDef("−", 6, 2, KeyStyle.WHITE, secondLabel = "]", alphaLabel = "W"),
                modifier = Modifier.weight(darkKeyWeight).fillMaxWidth(),
                onDown = { onKeyDown(6, 2) },
                onUp = { onKeyUp(6, 2) }
            )
            KeyButton(
                keyDef = KeyDef("+", 6, 1, KeyStyle.WHITE, secondLabel = "mem", alphaLabel = "\""),
                modifier = Modifier.weight(darkKeyWeight).fillMaxWidth(),
                onDown = { onKeyDown(6, 1) },
                onUp = { onKeyUp(6, 1) }
            )
            KeyButton(
                keyDef = KeyDef("enter", 6, 0, KeyStyle.BLUE, secondLabel = "entry", alphaLabel = "solve"),
                modifier = Modifier.weight(enterKeyWeight).fillMaxWidth(),
                onDown = { onKeyDown(6, 0) },
                onUp = { onKeyUp(6, 0) }
            )
        }
    }
}

@Composable
fun DPad(
    modifier: Modifier = Modifier,
    onKeyDown: (row: Int, col: Int) -> Unit,
    onKeyUp: (row: Int, col: Int) -> Unit
) {
    val sweepAngle = 90f
    val segmentColor = Color(0xFFE3E3E3)
    val pressedColor = Color(0xFFCECECE)
    val borderColor = Color(0xFFB5B5B5)
    val arrowColor = Color(0xFF2B2B2B)
    val gapColor = Color(0xFF1B1B1B)
    val gapWidthScale = 0.16f
    val innerRadiusScale = 0.45f

    val upAngle = 270f
    val leftAngle = 180f
    val rightAngle = 0f
    val downAngle = 90f

    var pressedDir by remember { mutableStateOf<DPadDirection?>(null) }

    Box(
        modifier = modifier.pointerInput(Unit) {
            detectTapGestures(
                onPress = { offset ->
                    val hitSize = Size(size.width.toFloat(), size.height.toFloat())
                    val hit = hitTestDPad(
                        offset = offset,
                        size = hitSize,
                        sweepAngle = sweepAngle,
                        innerRadiusScale = innerRadiusScale,
                        gapWidthScale = gapWidthScale
                    )
                    if (hit == null) {
                        return@detectTapGestures
                    }
                    pressedDir = hit
                    // CEmu: down(7,0), left(7,1), right(7,2), up(7,3)
                    when (hit) {
                        DPadDirection.UP -> onKeyDown(7, 3)
                        DPadDirection.LEFT -> onKeyDown(7, 1)
                        DPadDirection.RIGHT -> onKeyDown(7, 2)
                        DPadDirection.DOWN -> onKeyDown(7, 0)
                    }
                    try {
                        awaitRelease()
                    } finally {
                        when (hit) {
                            DPadDirection.UP -> onKeyUp(7, 3)
                            DPadDirection.LEFT -> onKeyUp(7, 1)
                            DPadDirection.RIGHT -> onKeyUp(7, 2)
                            DPadDirection.DOWN -> onKeyUp(7, 0)
                        }
                        pressedDir = null
                    }
                }
            )
        },
        contentAlignment = Alignment.Center
    ) {
        DPadSegment(
            startAngle = upAngle - sweepAngle / 2f,
            sweepAngle = sweepAngle,
            directionAngle = upAngle,
            innerRadiusScale = innerRadiusScale,
            gapWidthScale = gapWidthScale,
            fillColor = segmentColor,
            pressedColor = pressedColor,
            borderColor = borderColor,
            arrowColor = arrowColor,
            isPressed = pressedDir == DPadDirection.UP
        )
        DPadSegment(
            startAngle = leftAngle - sweepAngle / 2f,
            sweepAngle = sweepAngle,
            directionAngle = leftAngle,
            innerRadiusScale = innerRadiusScale,
            gapWidthScale = gapWidthScale,
            fillColor = segmentColor,
            pressedColor = pressedColor,
            borderColor = borderColor,
            arrowColor = arrowColor,
            isPressed = pressedDir == DPadDirection.LEFT
        )
        DPadSegment(
            startAngle = rightAngle - sweepAngle / 2f,
            sweepAngle = sweepAngle,
            directionAngle = rightAngle,
            innerRadiusScale = innerRadiusScale,
            gapWidthScale = gapWidthScale,
            fillColor = segmentColor,
            pressedColor = pressedColor,
            borderColor = borderColor,
            arrowColor = arrowColor,
            isPressed = pressedDir == DPadDirection.RIGHT
        )
        DPadSegment(
            startAngle = downAngle - sweepAngle / 2f,
            sweepAngle = sweepAngle,
            directionAngle = downAngle,
            innerRadiusScale = innerRadiusScale,
            gapWidthScale = gapWidthScale,
            fillColor = segmentColor,
            pressedColor = pressedColor,
            borderColor = borderColor,
            arrowColor = arrowColor,
            isPressed = pressedDir == DPadDirection.DOWN
        )

        DPadGaps(
            gapWidthScale = gapWidthScale,
            color = gapColor
        )

        Box(
            modifier = Modifier
                .size(32.dp)
                .background(Color(0xFF1B1B1B), CircleShape)
        )
    }
}

@Composable
fun KeyRow(
    keys: List<KeyDef>,
    modifier: Modifier = Modifier,
    onKeyDown: (row: Int, col: Int) -> Unit,
    onKeyUp: (row: Int, col: Int) -> Unit
) {
    Row(
        modifier = modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(2.dp)
    ) {
        keys.forEach { keyDef ->
            KeyButton(
                keyDef = keyDef,
                modifier = Modifier.weight(1f),
                onDown = { onKeyDown(keyDef.row, keyDef.col) },
                onUp = { onKeyUp(keyDef.row, keyDef.col) }
            )
        }
    }
}

@Composable
fun DPadSegment(
    startAngle: Float,
    sweepAngle: Float,
    directionAngle: Float,
    innerRadiusScale: Float,
    gapWidthScale: Float,
    fillColor: Color,
    pressedColor: Color,
    borderColor: Color,
    arrowColor: Color,
    isPressed: Boolean
) {
    Canvas(
        modifier = Modifier.fillMaxSize()
    ) {
        val outerRadius = min(size.width, size.height) / 2f
        val innerRadius = outerRadius * innerRadiusScale
        val strokeWidth = outerRadius * 0.035f
        val center = Offset(size.width / 2f, size.height / 2f)
        val outerRadiusAdjusted = outerRadius - strokeWidth * 0.2f
        val innerRadiusAdjusted = innerRadius + strokeWidth * 0.15f
        val outerRect = Rect(
            left = center.x - outerRadiusAdjusted,
            top = center.y - outerRadiusAdjusted,
            right = center.x + outerRadiusAdjusted,
            bottom = center.y + outerRadiusAdjusted
        )
        val innerRect = Rect(
            left = center.x - innerRadiusAdjusted,
            top = center.y - innerRadiusAdjusted,
            right = center.x + innerRadiusAdjusted,
            bottom = center.y + innerRadiusAdjusted
        )

        val segmentPath = Path().apply {
            arcTo(outerRect, startAngle, sweepAngle, false)
            arcTo(innerRect, startAngle + sweepAngle, -sweepAngle, false)
            close()
        }

        val activeFill = if (isPressed) pressedColor else fillColor
        val rimColor = if (isPressed) {
            blendColors(borderColor, Color.Black, 0.35f)
        } else {
            blendColors(borderColor, Color.White, 0.35f)
        }
        val innerRim = if (isPressed) {
            blendColors(activeFill, Color.Black, 0.15f)
        } else {
            blendColors(activeFill, Color.White, 0.18f)
        }

        drawPath(segmentPath, color = activeFill)
        drawPath(segmentPath, color = rimColor, style = Stroke(width = strokeWidth))
        drawPath(segmentPath, color = innerRim, style = Stroke(width = strokeWidth * 0.6f))

        val arrowRadius = (innerRadius + outerRadius) * 0.5f
        val arrowCenter = Offset(
            x = center.x + cos(directionAngle.toRadians()) * arrowRadius,
            y = center.y + sin(directionAngle.toRadians()) * arrowRadius
        )
        val arrowLength = outerRadius * 0.09f
        val arrowWidth = outerRadius * 0.16f
        val forward = Offset(
            x = cos(directionAngle.toRadians()),
            y = sin(directionAngle.toRadians())
        )
        val perpendicular = Offset(-forward.y, forward.x)
        val tip = arrowCenter + forward * arrowLength
        val baseCenter = arrowCenter - forward * (arrowLength * 0.45f)
        val left = baseCenter + perpendicular * (arrowWidth * 0.5f)
        val right = baseCenter - perpendicular * (arrowWidth * 0.5f)

        val arrowPath = Path().apply {
            moveTo(tip.x, tip.y)
            lineTo(left.x, left.y)
            lineTo(right.x, right.y)
            close()
        }
        drawPath(arrowPath, color = arrowColor)
    }
}

private enum class DPadDirection {
    UP,
    LEFT,
    RIGHT,
    DOWN
}

@Composable
fun DPadGaps(
    gapWidthScale: Float,
    color: Color,
    modifier: Modifier = Modifier
) {
    Canvas(modifier = modifier.fillMaxSize()) {
        val outerRadius = min(size.width, size.height) / 2f
        val gapWidth = outerRadius * gapWidthScale
        val rectLength = outerRadius * 2.1f
        val center = Offset(size.width / 2f, size.height / 2f)
        val rectTop = Offset(center.x - gapWidth / 2f, center.y - rectLength / 2f)
        val rectSize = Size(gapWidth, rectLength)
        rotate(45f, center) {
            drawRect(color = color, topLeft = rectTop, size = rectSize)
        }
        rotate(-45f, center) {
            drawRect(color = color, topLeft = rectTop, size = rectSize)
        }
    }
}

@Composable
fun KeyButton(
    keyDef: KeyDef,
    modifier: Modifier = Modifier,
    onDown: () -> Unit,
    onUp: () -> Unit
) {
    var isPressed by remember { mutableStateOf(false) }

    // Colors tuned to the TI-84 Plus CE image
    val baseColor = when (keyDef.style) {
        KeyStyle.YELLOW -> Color(0xFF6AB6E6) // 2nd key blue
        KeyStyle.GREEN -> Color(0xFF6DBE45)  // alpha key green
        KeyStyle.WHITE -> Color(0xFFE6E6E6)  // light gray keys
        KeyStyle.BLUE -> Color(0xFFDCDCDC)   // enter key (slightly darker light gray)
        KeyStyle.ARROW -> Color(0xFF4A4A4A)  // arrow keys
        KeyStyle.DARK -> Color(0xFF2D2D2D)   // dark keys
    }

    val textColor = when (keyDef.style) {
        KeyStyle.GREEN -> Color(0xFF1A1A1A)
        KeyStyle.WHITE, KeyStyle.BLUE -> Color(0xFF1A1A1A)
        else -> Color(0xFFF7F7F7)
    }

    val secondLabelColor = keyDef.secondLabelColor ?: Color(0xFF79C9FF)
    val alphaLabelColor = keyDef.alphaLabelColor ?: Color(0xFF7EC64B)
    val isNumberKey = isNumberClusterKey(keyDef.label)
    val keyShape = when (keyDef.style) {
        KeyStyle.WHITE, KeyStyle.BLUE -> if (isNumberKey) RoundedCornerShape(4.dp) else RoundedCornerShape(5.dp)
        KeyStyle.YELLOW, KeyStyle.GREEN -> RoundedCornerShape(7.dp)
        else -> RoundedCornerShape(6.dp)
    }
    val borderDarken = when (keyDef.style) {
        KeyStyle.WHITE, KeyStyle.BLUE -> 0.4f
        KeyStyle.DARK -> 0.48f
        else -> 0.35f
    }
    val borderWidth = if (keyDef.style == KeyStyle.WHITE || keyDef.style == KeyStyle.BLUE) 1.5.dp else 1.dp
    val borderColor = blendColors(baseColor, Color.Black, borderDarken)
    val topColor = blendColors(baseColor, Color.White, 0.16f)
    val bottomColor = blendColors(baseColor, Color.Black, 0.18f)
    val pressedTop = blendColors(baseColor, Color.Black, 0.22f)
    val pressedBottom = blendColors(baseColor, Color.Black, 0.32f)
    val keyBrush = Brush.verticalGradient(listOf(if (isPressed) pressedTop else topColor, if (isPressed) pressedBottom else bottomColor))

    Column(
        modifier = modifier
            .fillMaxHeight()
            .padding(horizontal = 1.dp),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        // Secondary labels above key: yellow (2nd) on left, green (alpha) on right - spaced apart
        val labelRowHeight = if (isNumberKey) 11.dp else 14.dp
        if (keyDef.secondLabel != null || keyDef.alphaLabel != null) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(labelRowHeight)
                    .padding(horizontal = 2.dp),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                Text(
                    text = keyDef.secondLabel ?: "",
                    color = secondLabelColor,
                    fontSize = 9.sp,
                    fontWeight = FontWeight.SemiBold,
                    fontFamily = FontFamily.SansSerif,
                    maxLines = 1
                )
                Text(
                    text = keyDef.alphaLabel ?: "",
                    color = alphaLabelColor,
                    fontSize = 9.sp,
                    fontWeight = FontWeight.SemiBold,
                    fontFamily = FontFamily.SansSerif,
                    maxLines = 1
                )
            }
        } else {
            Spacer(modifier = Modifier.height(labelRowHeight))
        }

        Spacer(modifier = Modifier.height(2.dp))

        // Main key button with border effect
        val mainFontSize = if (isNumberKey) 21.sp else 17.sp
        Box(
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .border(borderWidth, borderColor, keyShape)
                .background(keyBrush, keyShape)
                .pointerInput(Unit) {
                    detectTapGestures(
                        onPress = {
                            isPressed = true
                            onDown()
                            try {
                                awaitRelease()
                            } finally {
                                isPressed = false
                                onUp()
                            }
                        }
                    )
                },
            contentAlignment = Alignment.Center
        ) {
            Text(
                text = keyDef.label,
                color = textColor,
                fontSize = mainFontSize,
                fontWeight = if (keyDef.style == KeyStyle.WHITE || keyDef.style == KeyStyle.BLUE) FontWeight.Bold else FontWeight.SemiBold,
                fontFamily = FontFamily.SansSerif,
                maxLines = 1
            )
        }

        // Small spacer at bottom
        Spacer(modifier = Modifier.height(2.dp))
    }
}

private fun blendColors(base: Color, overlay: Color, ratio: Float): Color {
    val clamped = ratio.coerceIn(0f, 1f)
    return Color(
        red = base.red + (overlay.red - base.red) * clamped,
        green = base.green + (overlay.green - base.green) * clamped,
        blue = base.blue + (overlay.blue - base.blue) * clamped,
        alpha = base.alpha + (overlay.alpha - base.alpha) * clamped
    )
}

private fun isNumberClusterKey(label: String): Boolean {
    return (label.length == 1 && label[0].isDigit()) || label == "." || label == "(−)"
}

private fun isPointInSegment(
    point: Offset,
    size: Size,
    startAngle: Float,
    sweepAngle: Float,
    innerRadiusScale: Float,
    gapWidthScale: Float
): Boolean {
    val center = Offset(size.width / 2f, size.height / 2f)
    val dx = point.x - center.x
    val dy = point.y - center.y
    val radius = sqrt(dx * dx + dy * dy)
    val outerRadius = min(size.width, size.height) / 2f
    val innerRadius = outerRadius * innerRadiusScale
    if (radius < innerRadius || radius > outerRadius) {
        return false
    }
    if (isPointInGap(dx, dy, outerRadius, gapWidthScale)) {
        return false
    }
    val angle = (atan2(dy, dx) * 180f / PI).toFloat()
    val normalized = (angle + 360f) % 360f
    val end = (startAngle + sweepAngle) % 360f
    return if (sweepAngle >= 360f) {
        true
    } else if (startAngle <= end) {
        normalized in startAngle..end
    } else {
        normalized >= startAngle || normalized <= end
    }
}

private fun isPointInGap(dx: Float, dy: Float, outerRadius: Float, gapWidthScale: Float): Boolean {
    val gapWidth = outerRadius * gapWidthScale
    val threshold = gapWidth * 0.5f * 1.41421356f
    return abs(dy - dx) < threshold || abs(dy + dx) < threshold
}

private fun hitTestDPad(
    offset: Offset,
    size: Size,
    sweepAngle: Float,
    innerRadiusScale: Float,
    gapWidthScale: Float
): DPadDirection? {
    val upStart = 270f - sweepAngle / 2f
    val leftStart = 180f - sweepAngle / 2f
    val rightStart = 0f - sweepAngle / 2f
    val downStart = 90f - sweepAngle / 2f

    return when {
        isPointInSegment(offset, size, upStart, sweepAngle, innerRadiusScale, gapWidthScale) -> DPadDirection.UP
        isPointInSegment(offset, size, leftStart, sweepAngle, innerRadiusScale, gapWidthScale) -> DPadDirection.LEFT
        isPointInSegment(offset, size, rightStart, sweepAngle, innerRadiusScale, gapWidthScale) -> DPadDirection.RIGHT
        isPointInSegment(offset, size, downStart, sweepAngle, innerRadiusScale, gapWidthScale) -> DPadDirection.DOWN
        else -> null
    }
}

private fun Float.toRadians(): Float {
    return (this / 180f) * PI.toFloat()
}

private operator fun Offset.plus(other: Offset): Offset = Offset(x + other.x, y + other.y)
private operator fun Offset.minus(other: Offset): Offset = Offset(x - other.x, y - other.y)
private operator fun Offset.times(value: Float): Offset = Offset(x * value, y * value)
