#include <jni.h>
#include <android/log.h>
#include <cstring>
#include <deque>
#include <mutex>
#include <string>
#include <vector>

#include "emu.h"

#define LOG_TAG "EmuJNI"
#define LOGI(...) __android_log_print(ANDROID_LOG_INFO, LOG_TAG, __VA_ARGS__)
#define LOGE(...) __android_log_print(ANDROID_LOG_ERROR, LOG_TAG, __VA_ARGS__)

static std::mutex g_log_mutex;
static std::deque<std::string> g_logs;
static constexpr size_t kMaxLogs = 200;

static void emu_log_callback(const char* message) {
    if (message == nullptr) {
        return;
    }
    __android_log_print(ANDROID_LOG_INFO, "EmuCore", "%s", message);
    std::lock_guard<std::mutex> lock(g_log_mutex);
    g_logs.emplace_back(message);
    if (g_logs.size() > kMaxLogs) {
        g_logs.pop_front();
    }
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

JNIEXPORT jlong JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeCreate(JNIEnv* env, jobject thiz) {
    LOGI("========================================");
    LOGI("=== TI-84 CE Emulator Starting ===");
    LOGI("========================================");
    LOGI("Creating emulator instance");
    static bool log_set = false;
    if (!log_set) {
        emu_set_log_callback(emu_log_callback);
        log_set = true;
    }
    Emu* emu = emu_create();
    if (emu == nullptr) {
        LOGE("Failed to create emulator instance");
        return 0;
    }
    return fromEmu(emu);
}

JNIEXPORT void JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeDestroy(JNIEnv* env, jobject thiz, jlong handle) {
    LOGI("Destroying emulator instance");
    Emu* emu = toEmu(handle);
    if (emu != nullptr) {
        emu_destroy(emu);
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
    int result = emu_load_rom(emu, reinterpret_cast<const uint8_t*>(data), static_cast<size_t>(len));

    env->ReleaseByteArrayElements(romBytes, data, JNI_ABORT);

    if (result != 0) {
        LOGE("nativeLoadRom: emu_load_rom returned %d", result);
    }
    return result;
}

JNIEXPORT void JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeReset(JNIEnv* env, jobject thiz, jlong handle) {
    Emu* emu = toEmu(handle);
    if (emu != nullptr) {
        LOGI("Resetting emulator");
        emu_reset(emu);
    }
}

JNIEXPORT jint JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeRunCycles(JNIEnv* env, jobject thiz, jlong handle, jint cycles) {
    Emu* emu = toEmu(handle);
    if (emu == nullptr) {
        return 0;
    }
    return emu_run_cycles(emu, cycles);
}

JNIEXPORT jint JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeGetWidth(JNIEnv* env, jobject thiz, jlong handle) {
    Emu* emu = toEmu(handle);
    if (emu == nullptr) {
        return 0;
    }
    int w = 0, h = 0;
    emu_framebuffer(emu, &w, &h);
    return w;
}

JNIEXPORT jint JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeGetHeight(JNIEnv* env, jobject thiz, jlong handle) {
    Emu* emu = toEmu(handle);
    if (emu == nullptr) {
        return 0;
    }
    int w = 0, h = 0;
    emu_framebuffer(emu, &w, &h);
    return h;
}

JNIEXPORT jint JNICALL
Java_com_calc_emulator_EmulatorBridge_nativeCopyFramebuffer(JNIEnv* env, jobject thiz, jlong handle, jintArray outArgb) {
    Emu* emu = toEmu(handle);
    if (emu == nullptr) {
        LOGE("nativeCopyFramebuffer: null handle");
        return -1;
    }

    int w = 0, h = 0;
    const uint32_t* fb = emu_framebuffer(emu, &w, &h);
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
        LOGI("JNI setKey: row=%d col=%d down=%d", static_cast<int>(row), static_cast<int>(col), static_cast<int>(down));
        emu_set_key(emu, row, col, down ? 1 : 0);
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
    return static_cast<jlong>(emu_save_state_size(emu));
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

    int result = emu_save_state(emu, reinterpret_cast<uint8_t*>(data), static_cast<size_t>(cap));

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

    int result = emu_load_state(emu, reinterpret_cast<const uint8_t*>(data), static_cast<size_t>(len));

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

} // extern "C"
