#include <jni.h>
#include <stdint.h>

JNIEXPORT jlong JNICALL
Java_dev_emucore_probe_ProbeActivity_nativeChecksum(JNIEnv *env, jclass clazz, jint seed) {
    (void)env;
    (void)clazz;
    uint32_t value = (uint32_t)seed;
    for (unsigned index = 0; index < 64; index++) {
        value = value * UINT32_C(1664525) + UINT32_C(1013904223);
    }
    return (jlong)value;
}

JNIEXPORT jint JNICALL
Java_dev_emucore_probe_ProbeActivity_nativeAbiCode(JNIEnv *env, jclass clazz) {
    (void)env;
    (void)clazz;
#if defined(__x86_64__)
    return 1;
#elif defined(__aarch64__)
    return 2;
#else
#error Unsupported probe architecture
#endif
}
