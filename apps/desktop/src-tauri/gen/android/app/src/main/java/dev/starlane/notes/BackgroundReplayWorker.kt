package dev.starlane.notes

import android.content.Context
import androidx.work.Constraints
import androidx.work.Data
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequest
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import java.util.concurrent.TimeUnit

class BackgroundReplayWorker(
  appContext: Context,
  workerParams: WorkerParameters,
) : Worker(appContext, workerParams) {
  override fun doWork(): Result {
    val provider = inputData.getString(KEY_PROVIDER) ?: PROVIDER_NONE
    val wakeReason = inputData.getString(KEY_WAKE_REASON) ?: WAKE_REASON
    val now = System.currentTimeMillis()

    applicationContext
      .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
      .edit()
      .putString("last_provider", provider)
      .putString("last_wake_reason", wakeReason)
      .putLong("last_wake_epoch_millis", now)
      .putString("last_unique_work_name", uniqueWorkName(provider))
      .putString("last_wake_source", "shared-preferences")
      .apply()

    return Result.success(
      Data.Builder()
        .putString(KEY_PROVIDER, provider)
        .putString(KEY_WAKE_REASON, wakeReason)
        .putLong("wake_epoch_millis", now)
        .build(),
    )
  }

  companion object {
    const val PREFS_NAME = "starlane_background_replay_worker"
    const val KEY_PROVIDER = "provider"
    const val KEY_WAKE_REASON = "wake_reason"
    const val WAKE_REASON = "android-workmanager"
    const val PROVIDER_NONE = "none"
    private val DEFAULT_PROVIDERS = listOf(PROVIDER_NONE, "icloud-drive", "google-drive")

    fun registerDefaultSchedules(context: Context) {
      DEFAULT_PROVIDERS.forEach { provider ->
        registerProviderSchedule(context, provider)
      }
    }

    fun registerProviderSchedule(context: Context, provider: String) {
      val request = PeriodicWorkRequest.Builder(
        BackgroundReplayWorker::class.java,
        15,
        TimeUnit.MINUTES,
      )
        .setInputData(
          Data.Builder()
            .putString(KEY_PROVIDER, provider)
            .putString(KEY_WAKE_REASON, WAKE_REASON)
            .build(),
        )
        .setConstraints(
          Constraints.Builder()
            .setRequiredNetworkType(NetworkType.CONNECTED)
            .build(),
        )
        .addTag(uniqueWorkName(provider))
        .build()

      WorkManager.getInstance(context).enqueueUniquePeriodicWork(
        uniqueWorkName(provider),
        ExistingPeriodicWorkPolicy.UPDATE,
        request,
      )
    }

    fun uniqueWorkName(provider: String): String =
      "dev.starlane.notes.background-replay.${sanitizeProvider(provider)}"

    private fun sanitizeProvider(provider: String): String {
      val sanitized = provider
        .map { ch ->
          if (ch.isLetterOrDigit() || ch == '-' || ch == '_') ch else '_'
        }
        .joinToString("")
        .trim('_')
      return sanitized.ifEmpty { "default" }
    }
  }
}
