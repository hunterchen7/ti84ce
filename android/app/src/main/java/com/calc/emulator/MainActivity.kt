package com.calc.emulator

import android.graphics.Bitmap
import android.net.Uri
import android.os.Bundle
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
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

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        if (!emulator.create()) {
            Log.e(TAG, "Failed to create emulator")
        }

        setContent {
            TI84EmulatorTheme(darkTheme = true) {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background
                ) {
                    EmulatorScreen(emulator)
                }
            }
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        emulator.destroy()
    }
}

@Composable
fun EmulatorScreen(emulator: EmulatorBridge) {
    val context = LocalContext.current

    // Emulator state
    var isRunning by remember { mutableStateOf(false) }
    var romLoaded by remember { mutableStateOf(false) }
    var romName by remember { mutableStateOf<String?>(null) }
    var romSize by remember { mutableIntStateOf(0) }
    var loadError by remember { mutableStateOf<String?>(null) }

    // Debug info
    var totalCyclesExecuted by remember { mutableLongStateOf(0L) }
    var frameCounter by remember { mutableIntStateOf(0) }
    var showDebug by remember { mutableStateOf(false) }
    var lastKeyPress by remember { mutableStateOf("None") }
    val logLines = remember { mutableStateListOf<String>() }
    var isLcdOn by remember { mutableStateOf(true) }

    // Speed control (1x = 800K cycles, adjustable 1-10x)
    var speedMultiplier by remember { mutableStateOf(1f) } // Default 1x (real-time)

    // Framebuffer bitmap
    val bitmap = remember {
        Bitmap.createBitmap(
            emulator.getWidth(),
            emulator.getHeight(),
            Bitmap.Config.ARGB_8888
        )
    }

    // Track if we restored from saved state
    var restoredFromState by remember { mutableStateOf(false) }

    // Helper function to load ROM bytes into emulator
    fun loadRomBytes(romBytes: ByteArray, name: String, saveToStorage: Boolean = true, tryLoadState: Boolean = true) {
        romSize = romBytes.size
        val result = emulator.loadRom(romBytes)
        if (result == 0) {
            romLoaded = true
            romName = name
            loadError = null
            totalCyclesExecuted = 0L
            frameCounter = 0
            logLines.clear()
            Log.i("EmulatorScreen", "ROM loaded: ${romBytes.size} bytes")

            // Save to storage for next launch
            if (saveToStorage) {
                RomStorage.saveRom(context, romBytes, name)
            }

            // Try to restore saved emulator state
            if (tryLoadState) {
                RomStorage.loadState(context)?.let { stateBytes ->
                    val loadResult = emulator.loadState(stateBytes)
                    if (loadResult == 0) {
                        Log.i("EmulatorScreen", "Restored emulator state")
                        restoredFromState = true
                    } else {
                        Log.w("EmulatorScreen", "Failed to restore state: $loadResult")
                    }
                }
            }

            isRunning = true  // Start emulation
        } else {
            loadError = "Failed to load ROM (error: $result)"
            Log.e("EmulatorScreen", "Failed to load ROM: $result")
        }
    }

    // Helper function to save emulator state
    fun saveEmulatorState() {
        if (romLoaded) {
            emulator.saveState()?.let { stateBytes ->
                if (RomStorage.saveState(context, stateBytes)) {
                    Log.i("EmulatorScreen", "State saved: ${stateBytes.size} bytes")
                }
            }
        }
    }

    // Try to load saved ROM on first launch
    LaunchedEffect(Unit) {
        if (!romLoaded) {
            RomStorage.loadSavedRom(context)?.let { (bytes, name) ->
                loadRomBytes(bytes, name, saveToStorage = false, tryLoadState = true)
            }
        }
    }

    // Save state when app goes to background
    val lifecycleOwner = LocalLifecycleOwner.current
    DisposableEffect(lifecycleOwner) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_PAUSE) {
                Log.i("EmulatorScreen", "App paused - saving state")
                saveEmulatorState()
            }
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose {
            lifecycleOwner.lifecycle.removeObserver(observer)
        }
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
                    val name = uri.lastPathSegment ?: "ROM"
                    // Clear existing saved state when loading a new ROM from picker
                    RomStorage.clearSavedState(context)
                    loadRomBytes(romBytes, name, saveToStorage = true, tryLoadState = false)
                }
            } catch (e: Exception) {
                loadError = "Error: ${e.message}"
                Log.e("EmulatorScreen", "Error loading ROM", e)
            }
        }
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

    // Show ROM loading screen if no ROM loaded, otherwise show emulator
    if (!romLoaded) {
        RomLoadingScreen(
            onLoadRom = { romPicker.launch(arrayOf("*/*")) },
            loadError = loadError
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
            lastKeyPress = lastKeyPress,
            logs = logLines,
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
    loadError: String?
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
    @Suppress("UNUSED_PARAMETER") emulator: EmulatorBridge,
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
    lastKeyPress: String,
    logs: List<String>,
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
                drawerContainerColor = Color(0xFF1A1A2E)
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
                    text = "Speed: ${speedMultiplier.toInt()}x",
                    color = Color.White,
                    fontSize = 14.sp,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)
                )
                Slider(
                    value = speedMultiplier,
                    onValueChange = { onSpeedChange(it) },
                    valueRange = 1f..10f,
                    steps = 8,
                    modifier = Modifier.padding(horizontal = 16.dp),
                    colors = SliderDefaults.colors(
                        thumbColor = Color(0xFF4CAF50),
                        activeTrackColor = Color(0xFF4CAF50),
                        inactiveTrackColor = Color(0xFF333344)
                    )
                )

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
                    .padding(8.dp),
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                // Screen display
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .aspectRatio(320f / 240f)
                        .background(Color.Black, RoundedCornerShape(4.dp))
                        .padding(4.dp)
                ) {
                    // Only show framebuffer when LCD is on; otherwise show black (matching CEmu)
                    if (isLcdOn) {
                        Image(
                            bitmap = bitmap.asImageBitmap(),
                            contentDescription = "Emulator screen",
                            modifier = Modifier.fillMaxSize(),
                            contentScale = ContentScale.Fit,
                            filterQuality = FilterQuality.None
                        )
                    }
                }

                Spacer(modifier = Modifier.height(8.dp))

                // Keypad
                Keypad(
                    modifier = Modifier
                        .fillMaxWidth()
                        .weight(1f),
                    onKeyDown = onKeyDown,
                    onKeyUp = onKeyUp
                )
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
    @Suppress("UNUSED_PARAMETER") gapWidthScale: Float,
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
