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
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.FilterQuality
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.calc.emulator.ui.theme.TI84EmulatorTheme
import kotlinx.coroutines.*
import java.io.InputStream

class MainActivity : ComponentActivity() {
    companion object {
        private const val TAG = "MainActivity"
        const val CYCLES_PER_TICK = 10000
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
    val scope = rememberCoroutineScope()

    // Emulator state
    var isRunning by remember { mutableStateOf(false) }
    var romLoaded by remember { mutableStateOf(false) }
    var romName by remember { mutableStateOf<String?>(null) }

    // Framebuffer bitmap
    val bitmap = remember {
        Bitmap.createBitmap(
            emulator.getWidth(),
            emulator.getHeight(),
            Bitmap.Config.ARGB_8888
        )
    }
    var frameCounter by remember { mutableIntStateOf(0) }

    // ROM picker launcher
    val romPicker = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenDocument()
    ) { uri: Uri? ->
        uri?.let {
            try {
                val inputStream: InputStream? = context.contentResolver.openInputStream(uri)
                inputStream?.use { stream ->
                    val romBytes = stream.readBytes()
                    val result = emulator.loadRom(romBytes)
                    if (result == 0) {
                        romLoaded = true
                        romName = uri.lastPathSegment ?: "ROM"
                        Log.i("EmulatorScreen", "ROM loaded: ${romBytes.size} bytes")
                    } else {
                        Log.e("EmulatorScreen", "Failed to load ROM: $result")
                    }
                }
            } catch (e: Exception) {
                Log.e("EmulatorScreen", "Error loading ROM", e)
            }
        }
    }

    // Emulation loop
    LaunchedEffect(isRunning) {
        if (isRunning) {
            while (isRunning) {
                withContext(Dispatchers.Default) {
                    emulator.runCycles(MainActivity.CYCLES_PER_TICK)
                }
                frameCounter++
                delay(MainActivity.FRAME_INTERVAL_MS)
            }
        }
    }

    // Update framebuffer on each frame
    LaunchedEffect(frameCounter) {
        emulator.copyFramebufferToBitmap(bitmap)
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(8.dp),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        // Control buttons
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(bottom = 8.dp),
            horizontalArrangement = Arrangement.SpaceEvenly
        ) {
            Button(
                onClick = { romPicker.launch(arrayOf("*/*")) },
                colors = ButtonDefaults.buttonColors(
                    containerColor = if (romLoaded) Color(0xFF4CAF50) else MaterialTheme.colorScheme.primary
                )
            ) {
                Text(if (romLoaded) "ROM Loaded" else "Import ROM")
            }

            Button(
                onClick = { isRunning = !isRunning },
                colors = ButtonDefaults.buttonColors(
                    containerColor = if (isRunning) Color(0xFFFF5722) else Color(0xFF4CAF50)
                )
            ) {
                Text(if (isRunning) "Pause" else "Run")
            }

            Button(
                onClick = {
                    emulator.reset()
                    frameCounter++
                }
            ) {
                Text("Reset")
            }
        }

        // ROM info
        romName?.let {
            Text(
                text = "ROM: $it",
                fontSize = 12.sp,
                color = Color.Gray,
                modifier = Modifier.padding(bottom = 4.dp)
            )
        }

        // Screen display
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .aspectRatio(320f / 240f)
                .background(Color.Black, RoundedCornerShape(4.dp))
                .padding(4.dp)
        ) {
            Image(
                bitmap = bitmap.asImageBitmap(),
                contentDescription = "Emulator screen",
                modifier = Modifier.fillMaxSize(),
                contentScale = ContentScale.Fit,
                filterQuality = FilterQuality.None
            )
        }

        Spacer(modifier = Modifier.height(8.dp))

        // Keypad
        Keypad(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f),
            onKeyDown = { row, col ->
                emulator.setKey(row, col, true)
                frameCounter++
            },
            onKeyUp = { row, col ->
                emulator.setKey(row, col, false)
                frameCounter++
            }
        )
    }
}

@Composable
fun Keypad(
    modifier: Modifier = Modifier,
    onKeyDown: (row: Int, col: Int) -> Unit,
    onKeyUp: (row: Int, col: Int) -> Unit
) {
    // TI-84 style keypad layout (simplified for Milestone 1)
    // Row 0-3 are the main keys, using first 4 rows of the matrix
    val keyLabels = listOf(
        listOf("Y=", "WINDOW", "ZOOM", "TRACE", "GRAPH"),
        listOf("2nd", "MODE", "DEL", "ALPHA", "X,T"),
        listOf("STAT", "MATH", "APPS", "PRGM", "VARS"),
        listOf("CLEAR", "X^-1", "SIN", "COS", "TAN"),
        listOf("^", "X^2", ",", "(", ")"),
        listOf("7", "8", "9", "/", "LOG"),
        listOf("4", "5", "6", "*", "LN"),
        listOf("1", "2", "3", "-", "STO"),
        listOf("0", ".", "(-)", "+", "ENTER")
    )

    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(4.dp)
    ) {
        keyLabels.forEachIndexed { rowIndex, rowKeys ->
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .weight(1f),
                horizontalArrangement = Arrangement.spacedBy(4.dp)
            ) {
                rowKeys.forEachIndexed { colIndex, label ->
                    KeyButton(
                        label = label,
                        modifier = Modifier.weight(1f),
                        onDown = { onKeyDown(rowIndex % 8, colIndex % 8) },
                        onUp = { onKeyUp(rowIndex % 8, colIndex % 8) }
                    )
                }
            }
        }
    }
}

@Composable
fun KeyButton(
    label: String,
    modifier: Modifier = Modifier,
    onDown: () -> Unit,
    onUp: () -> Unit
) {
    var isPressed by remember { mutableStateOf(false) }

    val backgroundColor = when {
        label in listOf("2nd") -> Color(0xFFFFEB3B)
        label in listOf("ALPHA") -> Color(0xFF4CAF50)
        label in listOf("ENTER") -> Color(0xFF2196F3)
        label in listOf("CLEAR", "DEL") -> Color(0xFFFF5722)
        label.all { it.isDigit() || it == '.' } -> Color(0xFF424242)
        label in listOf("+", "-", "*", "/", "^") -> Color(0xFF616161)
        else -> Color(0xFF333333)
    }

    val textColor = when {
        label in listOf("2nd") -> Color.Black
        else -> Color.White
    }

    Box(
        modifier = modifier
            .fillMaxHeight()
            .background(
                if (isPressed) backgroundColor.copy(alpha = 0.6f) else backgroundColor,
                RoundedCornerShape(4.dp)
            )
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
            text = label,
            color = textColor,
            fontSize = if (label.length > 3) 10.sp else 12.sp
        )
    }
}
