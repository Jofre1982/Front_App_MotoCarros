package dev.dioxus.main

// Ajustes de la ventana de MotoYa. El workflow del APK copia este archivo al
// proyecto Gradle que genera `dx` y lo engancha en MainActivity.onCreate.
//
// Desde targetSdk 35 Android dibuja la app de borde a borde: sin esto el
// contenido del WebView queda debajo de la barra de estado (hora, bateria)
// y de los botones de navegacion. Se le pone al contenido un margen igual
// al de esas barras (y al del teclado, para que no tape los campos).

import android.app.Activity
import android.view.View
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

object MotoYaWindow {
    fun start(activity: Activity) {
        val content = activity.findViewById<View>(android.R.id.content) ?: return
        ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
            val bars = insets.getInsets(
                WindowInsetsCompat.Type.systemBars() or
                    WindowInsetsCompat.Type.displayCutout() or
                    WindowInsetsCompat.Type.ime()
            )
            view.setPadding(bars.left, bars.top, bars.right, bars.bottom)
            WindowInsetsCompat.CONSUMED
        }
    }
}
