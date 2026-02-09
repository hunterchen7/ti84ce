/**
 * JNI Loader with Dynamic Backend Support
 *
 * This file provides the JNI interface and dynamically loads backend .so files
 * at runtime. The app can switch between backends by calling setBackend().
 *
 * Available backends are determined at compile time by HAS_RUST_BACKEND and
 * HAS_CEMU_BACKEND defines.
 */

#include <jni.h>
#include <android/log.h>
#include <dlfcn.h>
#include <cstring>
#include <deque>
#include <mutex>
#include <string>
#include <vector>

#include "emu.h"

#define LOG_TAG "EmuJNI"
#define LOGI(...) __android_log_print(ANDROID_LOG_INFO, LOG_TAG, __VA_ARGS__)
#define LOGE(...) __android_log_print(ANDROID_LOG_ERROR, LOG_TAG, __VA_ARGS__)

// Backend function pointer types
typedef const char* (*backend_get_name_fn)();
typedef Emu* (*backend_create_fn)();
typedef void (*backend_destroy_fn)(Emu*);
typedef void (*backend_set_log_callback_fn)(emu_log_cb_t);
typedef int (*backend_load_rom_fn)(Emu*, const uint8_t*, size_t);
typedef void (*backend_reset_fn)(Emu*);
typedef void (*backend_power_on_fn)(Emu*);
typedef int (*backend_run_cycles_fn)(Emu*, int);
typedef const uint32_t* (*backend_framebuffer_fn)(const Emu*, int*, int*);
typedef void (*backend_set_key_fn)(Emu*, int, int, int);
typedef uint8_t (*backend_get_backlight_fn)(const Emu*);
typedef int (*backend_is_lcd_on_fn)(const Emu*);
typedef size_t (*backend_save_state_size_fn)(const Emu*);
typedef int (*backend_save_state_fn)(const Emu*, uint8_t*, size_t);
typedef int (*backend_load_state_fn)(Emu*, const uint8_t*, size_t);
typedef void (*backend_set_temp_dir_fn)(const char*);

// Backend interface structure
struct BackendInterface {
    void* handle = nullptr;
    std::string name;

    backend_get_name_fn get_name = nullptr;
    backend_create_fn create = nullptr;
    backend_destroy_fn destroy = nullptr;
    backend_set_log_callback_fn set_log_callback = nullptr;
    backend_load_rom_fn load_rom = nullptr;
    backend_reset_fn reset = nullptr;
    backend_power_on_fn power_on = nullptr;
    backend_run_cycles_fn run_cycles = nullptr;
    backend_framebuffer_fn framebuffer = nullptr;
    backend_set_key_fn set_key = nullptr;
    backend_get_backlight_fn get_backlight = nullptr;
    backend_is_lcd_on_fn is_lcd_on = nullptr;
    backend_save_state_size_fn save_state_size = nullptr;
    backend_save_state_fn save_state = nullptr;
    backend_load_state_fn load_state = nullptr;
    backend_set_temp_dir_fn set_temp_dir = nullptr;  // Optional

    bool isLoaded() const { return handle != nullptr; }

    void unload() {
        if (handle) {
            dlclose(handle);
            handle = nullptr;
        }
        name.clear();
    }
};

// Global state
static std::mutex g_mutex;
static BackendInterface g_backend;
static Emu* g_emu = nullptr;
static std::string g_native_lib_dir;
static std::string g_cache_dir;

// Log callback state
static std::mutex g_log_mutex;
static std::deque<std::string> g_logs;
static constexpr size_t kMaxLogs = 200;

static void emu_log_callback(const char* message) {
    if (message == nullptr) return;
    __android_log_print(ANDROID_LOG_INFO, "EmuCore", "%s", message);
    std::lock_guard<std::mutex> lock(g_log_mutex);
    g_logs.emplace_back(message);
    if (g_logs.size() > kMaxLogs) {
        g_logs.pop_front();
    }
}

// Load a backend by name
static bool loadBackend(const std::string& backendName) {
    // Use just the library name, not full path. Android's linker will find it
    // since System.loadLibrary already loaded it from the APK.
    std::string libName = "libemu_" + backendName + ".so";
    LOGI("Loading backend: %s (%s)", backendName.c_str(), libName.c_str());

    void* handle = dlopen(libName.c_str(), RTLD_NOW | RTLD_LOCAL);
    if (!handle) {
        LOGE("Failed to load backend %s: %s", backendName.c_str(), dlerror());
        return false;
    }

    // Load all function pointers
    BackendInterface newBackend;
    newBackend.handle = handle;
    newBackend.name = backendName;

    #define LOAD_FUNC(name) \
        newBackend.name = (backend_##name##_fn)dlsym(handle, "backend_" #name); \
        if (!newBackend.name) { \
            LOGE("Failed to load symbol backend_" #name ": %s", dlerror()); \
            dlclose(handle); \
            return false; \
        }

    LOAD_FUNC(get_name)
    LOAD_FUNC(create)
    LOAD_FUNC(destroy)
    LOAD_FUNC(set_log_callback)
    LOAD_FUNC(load_rom)
    LOAD_FUNC(reset)
    LOAD_FUNC(power_on)
    LOAD_FUNC(run_cycles)
    LOAD_FUNC(framebuffer)
    LOAD_FUNC(set_key)
    LOAD_FUNC(get_backlight)
    LOAD_FUNC(is_lcd_on)
    LOAD_FUNC(save_state_size)
    LOAD_FUNC(save_state)
    LOAD_FUNC(load_state)

    #undef LOAD_FUNC

    // Load optional symbols (don't fail if missing)
    newBackend.set_temp_dir = (backend_set_temp_dir_fn)dlsym(handle, "backend_set_temp_dir");

    // Unload previous backend
    g_backend.unload();
    g_backend = newBackend;

    // Set up log callback
    g_backend.set_log_callback(emu_log_callback);

    // Set temp directory if available
    if (g_backend.set_temp_dir && !g_cache_dir.empty()) {
        g_backend.set_temp_dir(g_cache_dir.c_str());
    }

    LOGI("Backend %s loaded successfully", g_backend.get_name());
    return true;
}

// Get list of available backends
static std::vector<std::string> getAvailableBackends() {
    std::vector<std::string> backends;

#ifdef HAS_RUST_BACKEND
    backends.push_back("rust");
#endif
#ifdef HAS_CEMU_BACKEND
    backends.push_back("cemu");
#endif

    return backends;
}

// Get default backend name
static std::string getDefaultBackend() {
#ifdef HAS_RUST_BACKEND
    return "rust";
#elif defined(HAS_CEMU_BACKEND)
    return "cemu";
#else
    return "";
#endif
}

extern "C" {

// Helper to convert jlong to Emu*
static inline Emu* toEmu(jlong handle) {
    return reinterpret_cast<Emu*>(handle);
}

// Helper to convert Emu* to jlong
static inline jlong fromEmu(Emu* emu) {
    return reinterpret_cast<jlong>(emu);
}

// Initialize with native library directory and cache directory
JNIEXPORT void JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeInit(JNIEnv* env, jobject thiz, jstring nativeLibDir, jstring cacheDir) {
    const char* dir = env->GetStringUTFChars(nativeLibDir, nullptr);
    g_native_lib_dir = dir;
    env->ReleaseStringUTFChars(nativeLibDir, dir);
    LOGI("Native library directory: %s", g_native_lib_dir.c_str());

    if (cacheDir != nullptr) {
        const char* cache = env->GetStringUTFChars(cacheDir, nullptr);
        g_cache_dir = cache;
        env->ReleaseStringUTFChars(cacheDir, cache);
        LOGI("Cache directory: %s", g_cache_dir.c_str());
    }
}

// Get available backends as string array
JNIEXPORT jobjectArray JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeGetAvailableBackends(JNIEnv* env, jobject thiz) {
    auto backends = getAvailableBackends();

    jclass stringClass = env->FindClass("java/lang/String");
    jobjectArray array = env->NewObjectArray(static_cast<jsize>(backends.size()), stringClass, nullptr);

    for (size_t i = 0; i < backends.size(); i++) {
        jstring str = env->NewStringUTF(backends[i].c_str());
        env->SetObjectArrayElement(array, static_cast<jsize>(i), str);
        env->DeleteLocalRef(str);
    }

    return array;
}

// Get current backend name (or null if none loaded)
JNIEXPORT jstring JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeGetCurrentBackend(JNIEnv* env, jobject thiz) {
    std::lock_guard<std::mutex> lock(g_mutex);
    if (!g_backend.isLoaded()) {
        return nullptr;
    }
    return env->NewStringUTF(g_backend.name.c_str());
}

// Set backend by name (destroys current emulator if any)
JNIEXPORT jboolean JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeSetBackend(JNIEnv* env, jobject thiz, jstring backendName) {
    const char* name = env->GetStringUTFChars(backendName, nullptr);
    std::string backendStr(name);
    env->ReleaseStringUTFChars(backendName, name);

    std::lock_guard<std::mutex> lock(g_mutex);

    // Destroy current emulator if any
    if (g_emu != nullptr && g_backend.isLoaded()) {
        g_backend.destroy(g_emu);
        g_emu = nullptr;
    }

    if (!loadBackend(backendStr)) {
        return JNI_FALSE;
    }

    return JNI_TRUE;
}

JNIEXPORT jlong JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeCreate(JNIEnv* env, jobject thiz) {
    LOGI("========================================");
    LOGI("=== TI-84 CE Emulator Starting ===");
    LOGI("========================================");

    std::lock_guard<std::mutex> lock(g_mutex);

    // Load default backend if none loaded
    if (!g_backend.isLoaded()) {
        std::string defaultBackend = getDefaultBackend();
        if (defaultBackend.empty()) {
            LOGE("No backends available!");
            return 0;
        }
        if (!loadBackend(defaultBackend)) {
            LOGE("Failed to load default backend: %s", defaultBackend.c_str());
            return 0;
        }
    }

    LOGI("Creating emulator instance with backend: %s", g_backend.name.c_str());
    Emu* emu = g_backend.create();
    if (emu == nullptr) {
        LOGE("Failed to create emulator instance");
        return 0;
    }
    g_emu = emu;
    return fromEmu(emu);
}

JNIEXPORT void JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeDestroy(JNIEnv* env, jobject thiz, jlong handle) {
    LOGI("Destroying emulator instance");
    Emu* emu = toEmu(handle);
    if (emu != nullptr) {
        std::lock_guard<std::mutex> lock(g_mutex);
        if (g_backend.isLoaded()) {
            g_backend.destroy(emu);
        }
        if (g_emu == emu) {
            g_emu = nullptr;
        }
    }
}

JNIEXPORT jint JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeLoadRom(JNIEnv* env, jobject thiz, jlong handle, jbyteArray romBytes) {
    Emu* emu = toEmu(handle);
    if (emu == nullptr) {
        LOGE("nativeLoadRom: null handle");
        return -1;
    }

    jsize len = env->GetArrayLength(romBytes);
    if (len <= 0) {
        LOGE("nativeLoadRom: empty ROM data");
        return -2;
    }

    jbyte* data = env->GetByteArrayElements(romBytes, nullptr);
    if (data == nullptr) {
        LOGE("nativeLoadRom: failed to get byte array");
        return -3;
    }

    LOGI("Loading ROM: %d bytes", static_cast<int>(len));

    std::lock_guard<std::mutex> lock(g_mutex);
    if (!g_backend.isLoaded()) {
        env->ReleaseByteArrayElements(romBytes, data, JNI_ABORT);
        return -4;
    }

    int result = g_backend.load_rom(emu, reinterpret_cast<const uint8_t*>(data), static_cast<size_t>(len));

    env->ReleaseByteArrayElements(romBytes, data, JNI_ABORT);

    if (result != 0) {
        LOGE("nativeLoadRom: load_rom returned %d", result);
    }
    return result;
}

JNIEXPORT void JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeReset(JNIEnv* env, jobject thiz, jlong handle) {
    Emu* emu = toEmu(handle);
    if (emu != nullptr) {
        std::lock_guard<std::mutex> lock(g_mutex);
        if (g_backend.isLoaded()) {
            LOGI("Resetting emulator");
            g_backend.reset(emu);
        }
    }
}

JNIEXPORT void JNICALL
Java_com_calc_emulator_EmulatorBridge_nativePowerOn(JNIEnv* env, jobject thiz, jlong handle) {
    Emu* emu = toEmu(handle);
    if (emu != nullptr) {
        std::lock_guard<std::mutex> lock(g_mutex);
        if (g_backend.isLoaded()) {
            LOGI("Powering on emulator");
            g_backend.power_on(emu);
        }
    }
}

JNIEXPORT jint JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeRunCycles(JNIEnv* env, jobject thiz, jlong handle, jint cycles) {
    Emu* emu = toEmu(handle);
    if (emu == nullptr) {
        return 0;
    }
    std::lock_guard<std::mutex> lock(g_mutex);
    if (!g_backend.isLoaded()) {
        return 0;
    }
    return g_backend.run_cycles(emu, cycles);
}

JNIEXPORT jint JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeGetWidth(JNIEnv* env, jobject thiz, jlong handle) {
    Emu* emu = toEmu(handle);
    if (emu == nullptr) {
        return 0;
    }
    std::lock_guard<std::mutex> lock(g_mutex);
    if (!g_backend.isLoaded()) {
        return 0;
    }
    int w = 0, h = 0;
    g_backend.framebuffer(emu, &w, &h);
    return w;
}

JNIEXPORT jint JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeGetHeight(JNIEnv* env, jobject thiz, jlong handle) {
    Emu* emu = toEmu(handle);
    if (emu == nullptr) {
        return 0;
    }
    std::lock_guard<std::mutex> lock(g_mutex);
    if (!g_backend.isLoaded()) {
        return 0;
    }
    int w = 0, h = 0;
    g_backend.framebuffer(emu, &w, &h);
    return h;
}

JNIEXPORT jint JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeCopyFramebuffer(JNIEnv* env, jobject thiz, jlong handle, jintArray outArgb) {
    Emu* emu = toEmu(handle);
    if (emu == nullptr) {
        LOGE("nativeCopyFramebuffer: null handle");
        return -1;
    }

    std::lock_guard<std::mutex> lock(g_mutex);
    if (!g_backend.isLoaded()) {
        return -4;
    }

    int w = 0, h = 0;
    const uint32_t* fb = g_backend.framebuffer(emu, &w, &h);
    if (fb == nullptr) {
        LOGE("nativeCopyFramebuffer: null framebuffer");
        return -2;
    }

    jsize arrayLen = env->GetArrayLength(outArgb);
    int pixelCount = w * h;
    if (arrayLen < pixelCount) {
        LOGE("nativeCopyFramebuffer: array too small (%d < %d)", static_cast<int>(arrayLen), pixelCount);
        return -3;
    }

    // Copy framebuffer to Java array
    env->SetIntArrayRegion(outArgb, 0, pixelCount, reinterpret_cast<const jint*>(fb));

    return 0;
}

JNIEXPORT void JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeSetKey(JNIEnv* env, jobject thiz, jlong handle, jint row, jint col, jboolean down) {
    Emu* emu = toEmu(handle);
    if (emu != nullptr) {
        std::lock_guard<std::mutex> lock(g_mutex);
        if (g_backend.isLoaded()) {
            LOGI("JNI setKey: row=%d col=%d down=%d", static_cast<int>(row), static_cast<int>(col), static_cast<int>(down));
            g_backend.set_key(emu, row, col, down ? 1 : 0);
        }
    } else {
        LOGE("JNI setKey: NULL emulator handle!");
    }
}

JNIEXPORT jlong JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeSaveStateSize(JNIEnv* env, jobject thiz, jlong handle) {
    Emu* emu = toEmu(handle);
    if (emu == nullptr) {
        return 0;
    }
    std::lock_guard<std::mutex> lock(g_mutex);
    if (!g_backend.isLoaded()) {
        return 0;
    }
    return static_cast<jlong>(g_backend.save_state_size(emu));
}

JNIEXPORT jint JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeSaveState(JNIEnv* env, jobject thiz, jlong handle, jbyteArray outData) {
    Emu* emu = toEmu(handle);
    if (emu == nullptr) {
        return -1;
    }

    jsize cap = env->GetArrayLength(outData);
    jbyte* data = env->GetByteArrayElements(outData, nullptr);
    if (data == nullptr) {
        return -2;
    }

    std::lock_guard<std::mutex> lock(g_mutex);
    if (!g_backend.isLoaded()) {
        env->ReleaseByteArrayElements(outData, data, JNI_ABORT);
        return -4;
    }

    int result = g_backend.save_state(emu, reinterpret_cast<uint8_t*>(data), static_cast<size_t>(cap));

    env->ReleaseByteArrayElements(outData, data, 0); // 0 to copy back changes

    return result;
}

JNIEXPORT jint JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeLoadState(JNIEnv* env, jobject thiz, jlong handle, jbyteArray stateData) {
    Emu* emu = toEmu(handle);
    if (emu == nullptr) {
        return -1;
    }

    jsize len = env->GetArrayLength(stateData);
    jbyte* data = env->GetByteArrayElements(stateData, nullptr);
    if (data == nullptr) {
        return -2;
    }

    std::lock_guard<std::mutex> lock(g_mutex);
    if (!g_backend.isLoaded()) {
        env->ReleaseByteArrayElements(stateData, data, JNI_ABORT);
        return -4;
    }

    int result = g_backend.load_state(emu, reinterpret_cast<const uint8_t*>(data), static_cast<size_t>(len));

    env->ReleaseByteArrayElements(stateData, data, JNI_ABORT);

    return result;
}

JNIEXPORT jobjectArray JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeDrainLogs(JNIEnv* env, jobject thiz, jlong handle) {
    (void)thiz;
    (void)handle;

    std::vector<std::string> logs;
    {
        std::lock_guard<std::mutex> lock(g_log_mutex);
        logs.assign(g_logs.begin(), g_logs.end());
        g_logs.clear();
    }

    jclass stringClass = env->FindClass("java/lang/String");
    if (stringClass == nullptr) {
        return nullptr;
    }

    jobjectArray array = env->NewObjectArray(static_cast<jsize>(logs.size()), stringClass, nullptr);
    if (array == nullptr) {
        return nullptr;
    }

    for (jsize i = 0; i < static_cast<jsize>(logs.size()); i++) {
        jstring str = env->NewStringUTF(logs[i].c_str());
        if (str == nullptr) {
            continue;
        }
        env->SetObjectArrayElement(array, i, str);
        env->DeleteLocalRef(str);
    }

    return array;
}

JNIEXPORT jint JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeGetBacklight(JNIEnv* env, jobject thiz, jlong handle) {
    Emu* emu = toEmu(handle);
    if (emu == nullptr) {
        return 0;
    }
    std::lock_guard<std::mutex> lock(g_mutex);
    if (!g_backend.isLoaded()) {
        return 0;
    }
    return static_cast<jint>(g_backend.get_backlight(emu));
}

JNIEXPORT jboolean JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeIsLcdOn(JNIEnv* env, jobject thiz, jlong handle) {
    Emu* emu = toEmu(handle);
    if (emu == nullptr) {
        return JNI_FALSE;
    }
    std::lock_guard<std::mutex> lock(g_mutex);
    if (!g_backend.isLoaded()) {
        return JNI_FALSE;
    }
    return g_backend.is_lcd_on(emu) ? JNI_TRUE : JNI_FALSE;
}

} // extern "C"
