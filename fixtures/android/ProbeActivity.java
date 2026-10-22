package dev.emucore.probe;

import android.app.Activity;
import android.graphics.Color;
import android.os.Bundle;
import android.util.Log;
import android.view.View;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.TextView;

/** Owned JNI and input fixture. No network, storage, account, or game dependency. */
public final class ProbeActivity extends Activity {
    private static final String TAG = "EmuCoreProbe";
    private static final int INITIAL_SEED = 0x13579bdf;
    private TextView status;
    private Button button;
    private String abi;
    private int touches;

    private static native long nativeChecksum(int seed);
    private static native int nativeAbiCode();

    private static long referenceChecksum(int seed) {
        long value = seed & 0xffffffffL;
        for (int index = 0; index < 64; index++) {
            value = (value * 1664525L + 1013904223L) & 0xffffffffL;
        }
        return value;
    }

    private TextView text(String content, int size, int color) {
        TextView view = new TextView(this);
        view.setText(content);
        view.setTextSize(size);
        view.setTextColor(color);
        view.setPadding(0, 0, 0, 24);
        return view;
    }

    @Override
    public void onCreate(Bundle state) {
        super.onCreate(state);
        LinearLayout layout = new LinearLayout(this);
        layout.setOrientation(LinearLayout.VERTICAL);
        int padding = Math.round(24 * getResources().getDisplayMetrics().density);
        layout.setPadding(padding, padding * 2, padding, padding);
        layout.setBackgroundColor(Color.rgb(17, 24, 39));
        layout.addView(text("EmuCore native probe", 24, Color.WHITE));
        layout.addView(text("Owned Android compatibility fixture\nJNI computation and touch input", 16, Color.LTGRAY));
        status = text("Loading native library...", 18, Color.rgb(134, 239, 172));
        layout.addView(status);
        button = new Button(this);
        button.setText("Run native check");
        button.setContentDescription("Run native check");
        layout.addView(button, new LinearLayout.LayoutParams(-1, -2));
        button.setOnClickListener(new View.OnClickListener() {
            @Override
            public void onClick(View view) {
                touches++;
                checkNative("TOUCH");
            }
        });
        setContentView(layout);
        try {
            System.loadLibrary("emucore_probe");
            int code = nativeAbiCode();
            if (code == 1) {
                abi = "x86_64";
            } else if (code == 2) {
                abi = "arm64-v8a";
            } else {
                fail("Unexpected native ABI code: " + code);
                return;
            }
            checkNative("READY");
        } catch (UnsatisfiedLinkError | SecurityException error) {
            fail(error.toString());
        }
    }

    private void checkNative(String event) {
        long expected = referenceChecksum(INITIAL_SEED + touches);
        long actual = nativeChecksum(INITIAL_SEED + touches);
        if (expected != actual) {
            fail("Checksum mismatch: expected " + expected + ", got " + actual);
            return;
        }
        status.setText("Native ABI: " + abi + "\nChecksum: " + actual
                + "\nChecks passed\nTouches: " + touches);
        Log.i(TAG, event + " abi=" + abi + " checksum=" + actual + " touches=" + touches);
    }

    private void fail(String message) {
        status.setTextColor(Color.rgb(252, 165, 165));
        status.setText("Native probe failed\n" + message);
        button.setEnabled(false);
        Log.e(TAG, "FAIL " + message);
    }
}
