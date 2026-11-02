package dev.dioxus.main

// Notificaciones push de MotoYa (Firebase Cloud Messaging). El workflow del
// APK copia este archivo al proyecto Gradle que genera `dx` solo cuando hay
// google-services.json (secreto GOOGLE_SERVICES_JSON).
//
// - Crea el canal "Viajes y mandados" con prioridad alta (el servidor manda
//   las notificaciones a `motoya_servicios`).
// - En Android 13+ pide el permiso de notificaciones.
// - Guarda el token de Firebase en files/motoya_push_token.txt; la parte en
//   Rust lo lee y lo registra en el servidor (POST /me/device-token).

import android.Manifest
import android.app.Activity
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import com.google.firebase.messaging.FirebaseMessaging
import com.google.firebase.messaging.FirebaseMessagingService
import java.io.File

object MotoYaPush {
    const val CHANNEL_ID = "motoya_servicios"
    private const val TOKEN_FILE = "motoya_push_token.txt"
    private const val PERMISSION_REQUEST = 7001

    fun start(activity: Activity) {
        createChannel(activity)
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(activity, Manifest.permission.POST_NOTIFICATIONS)
            != PackageManager.PERMISSION_GRANTED
        ) {
            ActivityCompat.requestPermissions(
                activity,
                arrayOf(Manifest.permission.POST_NOTIFICATIONS),
                PERMISSION_REQUEST,
            )
        }
        try {
            FirebaseMessaging.getInstance().token.addOnSuccessListener { token ->
                saveToken(activity, token)
            }
        } catch (e: Exception) {
            // Sin Firebase la app sigue igual, solo sin notificaciones.
        }
    }

    fun saveToken(context: Context, token: String) {
        try {
            File(context.filesDir, TOKEN_FILE).writeText(token)
        } catch (e: Exception) {
        }
    }

    private fun createChannel(context: Context) {
        if (Build.VERSION.SDK_INT >= 26) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Viajes y mandados",
                NotificationManager.IMPORTANCE_HIGH,
            )
            channel.description = "Avisos de viajes, mandados y motocargas"
            context.getSystemService(NotificationManager::class.java)
                ?.createNotificationChannel(channel)
        }
    }
}

class MotoYaMessagingService : FirebaseMessagingService() {
    override fun onNewToken(token: String) {
        MotoYaPush.saveToken(this, token)
    }
}
