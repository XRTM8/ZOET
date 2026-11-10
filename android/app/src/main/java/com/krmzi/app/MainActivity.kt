package com.krmzi.app

import android.annotation.SuppressLint
import android.app.PictureInPictureParams
import android.content.BroadcastReceiver
import android.content.ComponentCallbacks2
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ActivityInfo
import android.content.pm.ApplicationInfo
import android.content.res.Configuration
import android.graphics.Color
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaMetadata
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.util.Rational
import android.view.KeyEvent
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import android.webkit.*
import android.widget.FrameLayout
import androidx.activity.OnBackPressedCallback
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.webkit.WebViewAssetLoader
import java.io.ByteArrayInputStream
import java.net.HttpURLConnection
import java.net.URL
import org.json.JSONObject
import android.provider.Settings
import android.widget.Toast
import androidx.core.content.FileProvider
import java.io.File

class MainActivity : AppCompatActivity() {

    private companion object {
        const val TAG = "KrmziMainActivity"
        const val APP_URL = "https://appassets.androidplatform.net/assets/public/index.html"
        const val FALLBACK_VERSION_NAME = "2.9.1"
        const val FALLBACK_VERSION_CODE = 27L
    }

    private lateinit var webView: WebView
    private lateinit var container: FrameLayout
    private lateinit var assetLoader: WebViewAssetLoader
    private var customView: View? = null
    private var customViewCallback: WebChromeClient.CustomViewCallback? = null
    private var isVideoPlaying = false
    private var isDestroyed = false
    private var appChromeClient: WebChromeClient? = null
    private var mediaSession: MediaSession? = null
    private var audioManager: AudioManager? = null
    private var audioFocusRequest: AudioFocusRequest? = null
    private var isNoisyReceiverRegistered = false
    private val noisyReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) {
            if (intent?.action == AudioManager.ACTION_AUDIO_BECOMING_NOISY) {
                if (isVideoPlaying && ::webView.isInitialized) {
                    webView.evaluateJavascript("if (window.zoetTogglePlayer) window.zoetTogglePlayer();", null)
                }
            }
        }
    }

    /* ------------------------------------------------------------------ */
    /*  JavaScript bridge                                                  */
    /* ------------------------------------------------------------------ */

    inner class KrmziBridge {
        @JavascriptInterface
        fun isTelevision(): Boolean {
            val uiMode = (resources.configuration.uiMode and Configuration.UI_MODE_TYPE_MASK) == Configuration.UI_MODE_TYPE_TELEVISION
            val leanback = packageManager.hasSystemFeature(android.content.pm.PackageManager.FEATURE_LEANBACK)
            val tvFeature = packageManager.hasSystemFeature("android.hardware.type.television")
            val fireTv = packageManager.hasSystemFeature("amazon.hardware.fire_tv")
            val isLeanbackLaunch = intent?.categories?.contains(Intent.CATEGORY_LEANBACK_LAUNCHER) == true
            val isBoxWithoutTouch = !packageManager.hasSystemFeature(android.content.pm.PackageManager.FEATURE_TOUCHSCREEN) &&
                (resources.configuration.navigation == Configuration.NAVIGATION_DPAD || resources.configuration.orientation == Configuration.ORIENTATION_LANDSCAPE)
            return uiMode || leanback || tvFeature || fireTv || isLeanbackLaunch || isBoxWithoutTouch
        }

        @JavascriptInterface
        fun setVideoPlaying(playing: Boolean) {
            runOnUiThread {
                isVideoPlaying = playing
                try {
                    if (playing) {
                        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                        hideSystemBars(true)
                        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                            setPictureInPictureParams(
                                PictureInPictureParams.Builder()
                                    .setAspectRatio(Rational(16, 9))
                                    .build()
                            )
                        }
                        // Note: Chromium WebView manages audio focus internally for HTML5 video.
                        // Do not request manual audio focus here to prevent Xiaomi/MIUI focus collision pausing.
                        registerNoisyReceiver()
                        mediaSession?.let {
                            it.isActive = true
                            val state = PlaybackState.Builder()
                                .setActions(PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PAUSE or PlaybackState.ACTION_PLAY_PAUSE or PlaybackState.ACTION_FAST_FORWARD or PlaybackState.ACTION_REWIND)
                                .setState(PlaybackState.STATE_PLAYING, PlaybackState.PLAYBACK_POSITION_UNKNOWN, 1.0f)
                                .build()
                            it.setPlaybackState(state)
                        }
                    } else {
                        window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                        hideSystemBars(false)
                        unregisterNoisyReceiver()
                        mediaSession?.let {
                            val state = PlaybackState.Builder()
                                .setActions(PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PAUSE or PlaybackState.ACTION_PLAY_PAUSE or PlaybackState.ACTION_FAST_FORWARD or PlaybackState.ACTION_REWIND)
                                .setState(PlaybackState.STATE_PAUSED, PlaybackState.PLAYBACK_POSITION_UNKNOWN, 0.0f)
                                .build()
                            it.setPlaybackState(state)
                        }
                    }
                } catch (e: Exception) {
                    Log.w(TAG, "Unable to update playback flags: ${e.message}")
                }
            }
        }

        @JavascriptInterface
        fun updateMediaMetadata(title: String, series: String) {
            runOnUiThread {
                try {
                    mediaSession?.let {
                        val meta = MediaMetadata.Builder()
                            .putString(MediaMetadata.METADATA_KEY_TITLE, title)
                            .putString(MediaMetadata.METADATA_KEY_ALBUM, series)
                            .putString(MediaMetadata.METADATA_KEY_ARTIST, "ZOET")
                            .build()
                        it.setMetadata(meta)
                    }
                } catch (e: Exception) {
                    Log.w(TAG, "Metadata update error: ${e.message}")
                }
            }
        }

        @JavascriptInterface
        fun setLandscape(enable: Boolean) {
            runOnUiThread {
                try {
                    requestedOrientation = if (enable) {
                        hideSystemBars(true)
                        ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
                    } else {
                        hideSystemBars(false)
                        if ((resources.configuration.uiMode and Configuration.UI_MODE_TYPE_MASK) == Configuration.UI_MODE_TYPE_TELEVISION) ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE else ActivityInfo.SCREEN_ORIENTATION_USER_PORTRAIT
                    }
                } catch (e: Exception) {
                    Log.e(TAG, "Orientation error: ${e.message}")
                }
            }
        }

        @JavascriptInterface
        fun isNative(): Boolean = true

        @JavascriptInterface
        fun getAppVersion(): String {
            return try {
                val info = packageManager.getPackageInfo(packageName, 0)
                val versionName = info.versionName ?: FALLBACK_VERSION_NAME
                val versionCode = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                    info.longVersionCode
                } else {
                    @Suppress("DEPRECATION")
                    info.versionCode.toLong()
                }
                JSONObject().apply {
                    put("versionName", versionName)
                    put("versionCode", versionCode)
                }.toString()
            } catch (_: Exception) {
                """{"versionName":"$FALLBACK_VERSION_NAME","versionCode":$FALLBACK_VERSION_CODE}"""
            }
        }

        @JavascriptInterface
        fun checkForUpdates(): String {
            var connection: HttpURLConnection? = null
            return try {
                val url = URL("https://api.github.com/repos/XRTM8/ZOET/releases/latest")
                val conn = (url.openConnection() as HttpURLConnection).apply {
                    connectTimeout = 8_000
                    readTimeout = 8_000
                    setRequestProperty("User-Agent", "ZOET-Android")
                    setRequestProperty("Accept", "application/vnd.github.v3+json")
                }
                connection = conn
                val code = conn.responseCode
                if (code != 200) {
                    return JSONObject().apply {
                        put("hasUpdate", false)
                        put("error", "HTTP $code")
                        put("isSuccess", false)
                    }.toString()
                }

                val body = conn.inputStream.bufferedReader().use { it.readText() }
                val json = JSONObject(body)
                val tagName = json.optString("tag_name", "").replace("v", "").trim()
                val releaseName = json.optString("name", "تحديث جديد")
                val releaseNotes = json.optString("body", "")

                var apkUrl = ""
                val assets = json.optJSONArray("assets")
                if (assets != null) {
                    for (i in 0 until assets.length()) {
                        val asset = assets.optJSONObject(i) ?: continue
                        val name = asset.optString("name", "")
                        if (name.endsWith(".apk", ignoreCase = true)) {
                            apkUrl = asset.optString("browser_download_url", "")
                            break
                        }
                    }
                }
                if (apkUrl.isEmpty()) {
                    apkUrl = json.optString(
                        "html_url",
                        "https://github.com/XRTM8/ZOET/releases/latest"
                    )
                }

                val info = packageManager.getPackageInfo(packageName, 0)
                val currentVersion = (info.versionName ?: FALLBACK_VERSION_NAME)
                    .replace("v", "")
                    .trim()

                JSONObject().apply {
                    put("hasUpdate", isNewerVersion(currentVersion, tagName))
                    put("currentVersion", currentVersion)
                    put("latestVersion", tagName)
                    put("releaseName", releaseName)
                    put("changelog", releaseNotes)
                    put("downloadUrl", apkUrl)
                    put("isSuccess", true)
                }.toString()
            } catch (e: Exception) {
                Log.e(TAG, "Update check error: ${e.message}")
                JSONObject().apply {
                    put("hasUpdate", false)
                    put("error", e.message)
                    put("isSuccess", false)
                }.toString()
            } finally {
                connection?.disconnect()
            }
        }

        @JavascriptInterface
        fun openDownloadUrl(downloadUrl: String) {
            downloadAndInstall(downloadUrl)
        }

        @JavascriptInterface
        fun downloadAndInstall(downloadUrl: String) {
            runOnUiThread {
                try {
                    val uri = Uri.parse(downloadUrl)
                    if (uri.scheme != "https" && uri.scheme != "http") return@runOnUiThread

                    if (!downloadUrl.endsWith(".apk", ignoreCase = true)) {
                        startActivity(Intent(Intent.ACTION_VIEW, uri).apply {
                            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                        })
                        return@runOnUiThread
                    }

                    Toast.makeText(this@MainActivity, "جارٍ تنزيل التحديث والتثبيت...", Toast.LENGTH_LONG).show()

                    Thread {
                        try {
                            var currentUrl = downloadUrl
                            var finalConn: HttpURLConnection? = null
                            var redirects = 0
                            while (redirects < 5) {
                                val conn = (URL(currentUrl).openConnection() as HttpURLConnection).apply {
                                    connectTimeout = 15_000
                                    readTimeout = 60_000
                                    instanceFollowRedirects = true
                                    setRequestProperty("User-Agent", "ZOET-Android")
                                }
                                val code = conn.responseCode
                                if (code in 300..399) {
                                    val location = conn.getHeaderField("Location") ?: break
                                    currentUrl = location
                                    conn.disconnect()
                                    redirects++
                                } else {
                                    finalConn = conn
                                    break
                                }
                            }

                            val conn = finalConn
                            if (conn != null && conn.responseCode in 200..299) {
                                val updateFile = File(cacheDir, "zoet-update.apk")
                                if (updateFile.exists()) updateFile.delete()
                                conn.inputStream.use { input ->
                                    updateFile.outputStream().use { output ->
                                        input.copyTo(output)
                                    }
                                }
                                runOnUiThread {
                                    installApkFile(updateFile)
                                }
                            } else {
                                runOnUiThread {
                                    fallbackBrowserDownload(downloadUrl)
                                }
                            }
                        } catch (e: Exception) {
                            Log.e(TAG, "In-app download failed: ${e.message}", e)
                            runOnUiThread {
                                fallbackBrowserDownload(downloadUrl)
                            }
                        }
                    }.start()
                } catch (e: Exception) {
                    Log.e(TAG, "Failed to launch download URL: ${e.message}")
                }
            }
        }
    }

    private fun isNewerVersion(current: String, latest: String): Boolean {
        if (latest.isEmpty()) return false
        val currentParts = current.split(".").mapNotNull { it.toIntOrNull() }
        val latestParts = latest.split(".").mapNotNull { it.toIntOrNull() }
        val maxLength = maxOf(currentParts.size, latestParts.size)
        for (i in 0 until maxLength) {
            val c = currentParts.getOrElse(i) { 0 }
            val l = latestParts.getOrElse(i) { 0 }
            if (l > c) return true
            if (l < c) return false
        }
        return false
    }

    private fun installApkFile(file: File) {
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                if (!packageManager.canRequestPackageInstalls()) {
                    Toast.makeText(this, "يرجى السماح بتثبيت التطبيقات لتحديث ZOET", Toast.LENGTH_LONG).show()
                    val settingsIntent = Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES).apply {
                        data = Uri.parse("package:$packageName")
                        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    }
                    startActivity(settingsIntent)
                    return
                }
            }
            val apkUri = FileProvider.getUriForFile(this, "$packageName.fileprovider", file)
            val installIntent = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(apkUri, "application/vnd.android.package-archive")
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            startActivity(installIntent)
        } catch (e: Exception) {
            Log.e(TAG, "Install failed: ${e.message}", e)
            Toast.makeText(this, "تعذر فتح مثبت الحزمة تلقائياً", Toast.LENGTH_SHORT).show()
        }
    }

    private fun fallbackBrowserDownload(url: String) {
        try {
            if (!url.startsWith("http://") && !url.startsWith("https://")) {
                Toast.makeText(this, "تعذر تثبيت حزمة التحديث", Toast.LENGTH_SHORT).show()
                return
            }
            val uri = Uri.parse(url)
            startActivity(Intent(Intent.ACTION_VIEW, uri).apply {
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            })
        } catch (e: Exception) {
            Log.e(TAG, "Browser fallback failed: ${e.message}")
        }
    }

    /* ------------------------------------------------------------------ */
    /*  Lifecycle                                                          */
    /* ------------------------------------------------------------------ */

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        if ((resources.configuration.uiMode and Configuration.UI_MODE_TYPE_MASK) == Configuration.UI_MODE_TYPE_TELEVISION) {
            requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
        }
        configureWindow()

        container = FrameLayout(this).apply {
            layoutParams = ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT
            )
            setBackgroundColor(Color.parseColor("#08090f"))
        }

        assetLoader = WebViewAssetLoader.Builder()
            .setDomain("appassets.androidplatform.net")
            .addPathHandler("/assets/", WebViewAssetLoader.AssetsPathHandler(this))
            .build()

        createWebView()
        container.addView(webView)
        setContentView(container)

        configureWebView()
        setupMediaSession()
        setupBackHandler()
        webView.loadUrl(APP_URL)
        webView.requestFocus()
    }

    private fun configureWindow() {
        try {
            WindowCompat.setDecorFitsSystemWindows(window, false)
            window.statusBarColor = Color.TRANSPARENT
            window.navigationBarColor = Color.BLACK
            window.addFlags(WindowManager.LayoutParams.FLAG_DRAWS_SYSTEM_BAR_BACKGROUNDS)
            window.setFlags(
                WindowManager.LayoutParams.FLAG_HARDWARE_ACCELERATED,
                WindowManager.LayoutParams.FLAG_HARDWARE_ACCELERATED
            )
            @Suppress("DEPRECATION")
            window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE)

            // Utilize full display under cutout/notch
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                window.attributes.layoutInDisplayCutoutMode =
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
            }

            // Enable 120Hz / 90Hz High Refresh Rate on supported displays
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                val display = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                    display
                } else {
                    @Suppress("DEPRECATION")
                    windowManager.defaultDisplay
                }
                val modes = display?.supportedModes
                val currentMode = display?.mode
                val highestMode = modes?.filter { it.physicalWidth == currentMode?.physicalWidth && it.physicalHeight == currentMode.physicalHeight }?.maxByOrNull { it.refreshRate }
                if (highestMode != null && highestMode.refreshRate > 60f) {
                    val params = window.attributes
                    params.preferredDisplayModeId = highestMode.modeId
                    window.attributes = params
                    Log.d(TAG, "Display set to high refresh rate: ${highestMode.refreshRate}Hz")
                }
            }
        } catch (e: Exception) {
            Log.w(TAG, "Window configuration failed: ${e.message}")
        }
    }

    private fun setupMediaSession() {
        try {
            audioManager = getSystemService(Context.AUDIO_SERVICE) as? AudioManager
            mediaSession = MediaSession(this, "ZOET_Playback").apply {
                @Suppress("DEPRECATION")
                setFlags(MediaSession.FLAG_HANDLES_MEDIA_BUTTONS or MediaSession.FLAG_HANDLES_TRANSPORT_CONTROLS)
                setCallback(object : MediaSession.Callback() {
                    override fun onPlay() {
                        runOnUiThread {
                            if (::webView.isInitialized) {
                                webView.evaluateJavascript("if (window.zoetTogglePlayer) window.zoetTogglePlayer(); else if (window.krmziTogglePlayer) window.krmziTogglePlayer();", null)
                            }
                        }
                    }
                    override fun onPause() {
                        runOnUiThread {
                            if (::webView.isInitialized) {
                                webView.evaluateJavascript("if (window.zoetTogglePlayer) window.zoetTogglePlayer(); else if (window.krmziTogglePlayer) window.krmziTogglePlayer();", null)
                            }
                        }
                    }
                    override fun onFastForward() {
                        runOnUiThread {
                            if (::webView.isInitialized) {
                                webView.evaluateJavascript("if (window.zoetSeekPlayer) window.zoetSeekPlayer(10); else if (window.krmziSeekPlayer) window.krmziSeekPlayer(10);", null)
                            }
                        }
                    }
                    override fun onRewind() {
                        runOnUiThread {
                            if (::webView.isInitialized) {
                                webView.evaluateJavascript("if (window.zoetSeekPlayer) window.zoetSeekPlayer(-10); else if (window.krmziSeekPlayer) window.krmziSeekPlayer(-10);", null)
                            }
                        }
                    }
                })
            }
        } catch (e: Exception) {
            Log.w(TAG, "MediaSession setup failed: ${e.message}")
        }
    }

    private fun requestPlaybackAudioFocus(): Boolean {
        val am = audioManager ?: return false
        return try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                val playbackAttributes = AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_MEDIA)
                    .setContentType(AudioAttributes.CONTENT_TYPE_MOVIE)
                    .build()
                val afr = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
                    .setAudioAttributes(playbackAttributes)
                    .setAcceptsDelayedFocusGain(false)
                    .setOnAudioFocusChangeListener { focusChange ->
                        if (focusChange == AudioManager.AUDIOFOCUS_LOSS) {
                            runOnUiThread {
                                if (isVideoPlaying && ::webView.isInitialized) {
                                    webView.evaluateJavascript("if (window.zoetTogglePlayer) { if (!document.getElementById('nativePlayer')?.paused) window.zoetTogglePlayer(); }", null)
                                }
                            }
                        }
                    }
                    .build()
                audioFocusRequest = afr
                am.requestAudioFocus(afr) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
            } else {
                @Suppress("DEPRECATION")
                am.requestAudioFocus(
                    { focusChange ->
                        if (focusChange == AudioManager.AUDIOFOCUS_LOSS) {
                            runOnUiThread {
                                if (isVideoPlaying && ::webView.isInitialized) {
                                    webView.evaluateJavascript("if (window.zoetTogglePlayer) { if (!document.getElementById('nativePlayer')?.paused) window.zoetTogglePlayer(); }", null)
                                }
                            }
                        }
                    },
                    AudioManager.STREAM_MUSIC,
                    AudioManager.AUDIOFOCUS_GAIN
                ) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
            }
        } catch (e: Exception) {
            Log.w(TAG, "Audio focus request failed: ${e.message}")
            false
        }
    }

    private fun abandonPlaybackAudioFocus() {
        val am = audioManager ?: return
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                audioFocusRequest?.let { am.abandonAudioFocusRequest(it) }
                audioFocusRequest = null
            } else {
                @Suppress("DEPRECATION")
                am.abandonAudioFocus(null)
            }
        } catch (e: Exception) {
            Log.w(TAG, "Abandon audio focus failed: ${e.message}")
        }
    }

    private fun registerNoisyReceiver() {
        if (!isNoisyReceiverRegistered) {
            try {
                registerReceiver(noisyReceiver, IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY))
                isNoisyReceiverRegistered = true
            } catch (e: Exception) {
                Log.w(TAG, "Noisy receiver registration failed: ${e.message}")
            }
        }
    }

    private fun unregisterNoisyReceiver() {
        if (isNoisyReceiverRegistered) {
            try {
                unregisterReceiver(noisyReceiver)
            } catch (_: Exception) {
            }
            isNoisyReceiverRegistered = false
        }
    }

    @SuppressLint("SetJavaScriptEnabled")
    private fun createWebView() {
        webView = WebView(this).apply {
            layoutParams = ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT
            )
            setBackgroundColor(Color.TRANSPARENT)
            setLayerType(View.LAYER_TYPE_NONE, null)
            overScrollMode = View.OVER_SCROLL_NEVER
            isVerticalScrollBarEnabled = false
            isHorizontalScrollBarEnabled = false
            isFocusable = true
            isFocusableInTouchMode = true
        }
    }

    @SuppressLint("SetJavaScriptEnabled")
    private fun configureWebView() {
        val settings = webView.settings
        settings.javaScriptEnabled = true
        settings.domStorageEnabled = true
        settings.databaseEnabled = true
        settings.allowFileAccess = false
        settings.allowContentAccess = false
        settings.loadsImagesAutomatically = true
        settings.blockNetworkImage = false
        settings.mediaPlaybackRequiresUserGesture = false
        settings.setSupportMultipleWindows(false)
        settings.javaScriptCanOpenWindowsAutomatically = false
        settings.cacheMode = WebSettings.LOAD_DEFAULT
        settings.useWideViewPort = true
        settings.loadWithOverviewMode = true
        settings.mixedContentMode = WebSettings.MIXED_CONTENT_ALWAYS_ALLOW
        settings.builtInZoomControls = false
        settings.displayZoomControls = false
        settings.setSupportZoom(false)
        settings.setGeolocationEnabled(false)
        settings.setNeedInitialFocus(false)
        settings.textZoom = 100
        settings.userAgentString = settings.userAgentString ?: "Mozilla/5.0 (Linux; Android 14; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36"
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            settings.offscreenPreRaster = true
        }

        try {
            val debugBuild = (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE) != 0
            WebView.setWebContentsDebuggingEnabled(debugBuild)
        } catch (_: Exception) {
        }

        // Cookies are required by several embedded video providers.
        try {
            val cookieManager = CookieManager.getInstance()
            cookieManager.setAcceptCookie(true)
            cookieManager.setAcceptThirdPartyCookies(webView, true)
        } catch (e: Exception) {
            Log.w(TAG, "Cookie configuration failed: ${e.message}")
        }

        webView.addJavascriptInterface(KrmziBridge(), "KrmziBridge")

        val chromeClient = object : WebChromeClient() {
            override fun onCreateWindow(
                view: WebView?,
                isDialog: Boolean,
                isUserGesture: Boolean,
                resultMsg: android.os.Message?
            ): Boolean {
                // Third-party ad networks must never open popups.
                return false
            }

            override fun onShowCustomView(view: View?, callback: CustomViewCallback?) {
                if (view == null) {
                    callback?.onCustomViewHidden()
                    return
                }
                if (customView != null) {
                    callback?.onCustomViewHidden()
                    return
                }
                customView = view
                customViewCallback = callback
                webView.visibility = View.GONE
                (view.parent as? ViewGroup)?.removeView(view)
                container.addView(
                    view,
                    FrameLayout.LayoutParams(
                        FrameLayout.LayoutParams.MATCH_PARENT,
                        FrameLayout.LayoutParams.MATCH_PARENT
                    )
                )
                hideSystemBars(true)
            }

            override fun onHideCustomView() {
                val view = customView ?: return
                webView.visibility = View.VISIBLE
                container.removeView(view)
                customView = null
                customViewCallback?.onCustomViewHidden()
                customViewCallback = null
                hideSystemBars(false)
            }

            override fun onConsoleMessage(message: ConsoleMessage?): Boolean {
                val msg = message ?: return true
                if (msg.messageLevel() == ConsoleMessage.MessageLevel.ERROR) {
                    Log.w(TAG, "WebView console: ${msg.message()} @${msg.lineNumber()}")
                }
                return true
            }
        }

        webView.webChromeClient = chromeClient
        appChromeClient = chromeClient
        webView.webViewClient = object : WebViewClient() {
            override fun shouldInterceptRequest(
                view: WebView?,
                request: WebResourceRequest?
            ): WebResourceResponse? {
                val requestUri = request?.url ?: return null
                val host = requestUri.host?.lowercase() ?: ""
                val path = requestUri.path ?: ""
                val isLocalApp = host == "appassets.androidplatform.net" ||
                    host == "localhost" ||
                    host == "127.0.0.1"

                if (isLocalApp && path == "/assets/public/wasm/otium_ui_bg.wasm") {
                    return WebResourceResponse("application/wasm", null, assets.open("public/wasm/otium_ui_bg.wasm"))
                }

                if (isLocalApp && path == "/api/cinemana/subtitle") {
                    return RustApi.response(requestUri.toString())
                }
                if (isLocalApp && path == "/api/media") {
                    return RustApi.response(requestUri.toString(), request.requestHeaders["Range"] ?: request.requestHeaders["range"] ?: "")
                }

                if (isLocalApp && path.startsWith("/api/")) {
                    if (request.method.equals("OPTIONS", ignoreCase = true)) {
                        return corsResponse(204, "No Content", "")
                    }
                    return try {
                        val json = RustApi.request(requestUri.toString())
                        WebResourceResponse(
                            "application/json",
                            "UTF-8",
                            200,
                            "OK",
                            corsHeaders(),
                            json.byteInputStream(Charsets.UTF_8)
                        )
                    } catch (e: Exception) {
                        Log.e(TAG, "Scraper interception error: ${e.message}", e)
                        val body = JSONObject()
                            .put("error", e.message ?: "Internal error")
                            .toString()
                        WebResourceResponse(
                            "application/json",
                            "UTF-8",
                            500,
                            "Internal Error",
                            corsHeaders(),
                            body.byteInputStream(Charsets.UTF_8)
                        )
                    }
                }

                val assetResponse = assetLoader.shouldInterceptRequest(requestUri)
                if (assetResponse != null) return assetResponse

                return super.shouldInterceptRequest(view, request)
            }

            override fun shouldOverrideUrlLoading(
                view: WebView?,
                request: WebResourceRequest?
            ): Boolean {
                val url = request?.url?.toString()?.lowercase() ?: return true

                if (url.startsWith("https://appassets.androidplatform.net") ||
                    url.startsWith("file://") ||
                    url.startsWith("about:blank")
                ) {
                    return false
                }

                // Video players and CDNs live in subframes; never block those.
                if (!request.isForMainFrame) return false

                Log.d(TAG, "Prevented external navigation on main frame: $url")
                return true
            }

            override fun onPageFinished(view: WebView?, url: String?) {
                super.onPageFinished(view, url)
                view?.evaluateJavascript(
                    "document.documentElement.classList.add('krmzi-native');",
                    null
                )
            }

            override fun onReceivedError(
                view: WebView?,
                request: WebResourceRequest?,
                error: WebResourceError?
            ) {
                super.onReceivedError(view, request, error)
                if (request?.isForMainFrame == true) {
                    Log.w(TAG, "Main frame error: ${error?.description}")
                }
            }

            override fun onReceivedHttpError(
                view: WebView?,
                request: WebResourceRequest?,
                errorResponse: WebResourceResponse?
            ) {
                super.onReceivedHttpError(view, request, errorResponse)
                if (request?.isForMainFrame == true) {
                    Log.w(TAG, "Main frame HTTP ${errorResponse?.statusCode} for ${request.url}")
                }
            }

            override fun onRenderProcessGone(
                view: WebView?,
                detail: RenderProcessGoneDetail?
            ): Boolean {
                val crashed = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) detail?.didCrash() else null
                Log.e(TAG, "WebView renderer gone (crashed=$crashed)")
                if (!isDestroyed) recreateWebView()
                return true
            }
        }
    }

    private fun corsHeaders(): MutableMap<String, String> = mutableMapOf(
        "Access-Control-Allow-Origin" to "*",
        "Access-Control-Allow-Methods" to "GET, POST, OPTIONS",
        "Access-Control-Allow-Headers" to "*",
        "Cache-Control" to "no-store",
        "X-Krmzi-Native" to "1"
    )

    private fun corsResponse(status: Int, reason: String, body: String): WebResourceResponse =
        WebResourceResponse(
            "application/json",
            "UTF-8",
            status,
            reason,
            corsHeaders(),
            ByteArrayInputStream(body.toByteArray(Charsets.UTF_8))
        )

    private fun recreateWebView() {
        try {
            container.removeView(webView)
            webView.stopLoading()
            webView.removeJavascriptInterface("KrmziBridge")
            webView.destroy()
        } catch (e: Exception) {
            Log.w(TAG, "Failed to destroy crashed WebView: ${e.message}")
        }

        customView = null
        customViewCallback = null
        isVideoPlaying = false

        createWebView()
        container.addView(webView)
        configureWebView()
        webView.loadUrl(APP_URL)
    }

    /* ------------------------------------------------------------------ */
    /*  Navigation & Key Dispatch                                          */
    /* ------------------------------------------------------------------ */

    private var lastBackPressTime: Long = 0L

    private fun setupBackHandler() {
        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                if (customView != null) {
                    appChromeClient?.onHideCustomView()
                    return
                }
                if (!::webView.isInitialized || isDestroyed) {
                    finish()
                    return
                }

                webView.evaluateJavascript(
                    """
                    (function() {
                        if (window.zoetHandleBack && window.zoetHandleBack()) return true;
                        if (window.krmziHandleBack && window.krmziHandleBack()) return true;
                        if (window.krmziIsSheetOpen && window.krmziIsSheetOpen()) {
                            window.krmziCloseSheet();
                            return true;
                        }
                        if (window.krmziIsPlayerOpen && window.krmziIsPlayerOpen()) {
                            window.krmziClosePlayer();
                            return true;
                        }
                        if (window.krmziIsSearchOpen && window.krmziIsSearchOpen()) {
                            window.krmziCloseSearch();
                            return true;
                        }
                        return false;
                    })()
                    """.trimIndent()
                ) { result ->
                    if (result != "true") {
                        val currentTime = System.currentTimeMillis()
                        if (currentTime - lastBackPressTime < 2000L) {
                            finish()
                        } else {
                            lastBackPressTime = currentTime
                            Toast.makeText(this@MainActivity, "اضغط مرة أخرى للخروج", Toast.LENGTH_SHORT).show()
                        }
                    }
                }
            }
        })
    }

    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        if (event.action == KeyEvent.ACTION_DOWN) {
            when (event.keyCode) {
                KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE,
                KeyEvent.KEYCODE_MEDIA_PLAY,
                KeyEvent.KEYCODE_MEDIA_PAUSE -> {
                    if (isVideoPlaying && ::webView.isInitialized) {
                        webView.evaluateJavascript("if (window.zoetTogglePlayer) window.zoetTogglePlayer(); else if (window.krmziTogglePlayer) window.krmziTogglePlayer();", null)
                        return true
                    }
                }
                KeyEvent.KEYCODE_MEDIA_FAST_FORWARD,
                KeyEvent.KEYCODE_MEDIA_STEP_FORWARD -> {
                    if (isVideoPlaying && ::webView.isInitialized) {
                        webView.evaluateJavascript("if (window.zoetSeekPlayer) window.zoetSeekPlayer(10); else if (window.krmziSeekPlayer) window.krmziSeekPlayer(10);", null)
                        return true
                    }
                }
                KeyEvent.KEYCODE_MEDIA_REWIND,
                KeyEvent.KEYCODE_MEDIA_STEP_BACKWARD -> {
                    if (isVideoPlaying && ::webView.isInitialized) {
                        webView.evaluateJavascript("if (window.zoetSeekPlayer) window.zoetSeekPlayer(-10); else if (window.krmziSeekPlayer) window.krmziSeekPlayer(-10);", null)
                        return true
                    }
                }
            }
        }
        return super.dispatchKeyEvent(event)
    }

    /* ------------------------------------------------------------------ */
    /*  Picture-in-picture, insets and memory                              */
    /* ------------------------------------------------------------------ */

    override fun onUserLeaveHint() {
        super.onUserLeaveHint()
        if (isVideoPlaying && Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            try {
                if (!isInPictureInPictureMode) {
                    enterPictureInPictureMode(
                        PictureInPictureParams.Builder()
                            .setAspectRatio(Rational(16, 9))
                            .build()
                    )
                }
            } catch (e: Exception) {
                Log.w(TAG, "PiP error: ${e.message}")
            }
        }
    }

    override fun onPictureInPictureModeChanged(
        isInPictureInPictureMode: Boolean,
        newConfig: Configuration
    ) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) super.onPictureInPictureModeChanged(isInPictureInPictureMode, newConfig)
        if (!::webView.isInitialized || isDestroyed) return
        val script = if (isInPictureInPictureMode) {
            "document.getElementById('playerHudTop')?.classList.add('hud-hidden');" +
                "document.getElementById('playerHudBottom')?.classList.add('hud-hidden');"
        } else {
            "document.getElementById('playerHudTop')?.classList.remove('hud-hidden');" +
                "document.getElementById('playerHudBottom')?.classList.remove('hud-hidden');"
        }
        webView.evaluateJavascript(script, null)
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) hideSystemBars(isVideoPlaying || customView != null)
    }

    private fun hideSystemBars(fullscreen: Boolean) {
        try {
            val controller = WindowCompat.getInsetsController(window, window.decorView)
            controller.systemBarsBehavior =
                WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            if (fullscreen) {
                controller.hide(WindowInsetsCompat.Type.systemBars())
            } else {
                controller.hide(WindowInsetsCompat.Type.statusBars())
            }
        } catch (e: Exception) {
            Log.w(TAG, "Unable to update system bars: ${e.message}")
        }
    }

    override fun onPause() {
        val inPip = Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && isInPictureInPictureMode
        if (::webView.isInitialized && !inPip) {
            webView.onPause()
            webView.pauseTimers()
        }
        try {
            CookieManager.getInstance().flush()
        } catch (_: Exception) {
        }
        super.onPause()
    }

    override fun onResume() {
        super.onResume()
        if (::webView.isInitialized) {
            webView.onResume()
            webView.resumeTimers()
        }
    }

    override fun onLowMemory() {
        super.onLowMemory()
        if (::webView.isInitialized) webView.clearCache(false)
        RustApi.trimMemory()
    }

    override fun onTrimMemory(level: Int) {
        super.onTrimMemory(level)
        if (level >= ComponentCallbacks2.TRIM_MEMORY_RUNNING_LOW) {
            if (::webView.isInitialized) webView.clearCache(false)
            RustApi.trimMemory()
        }
    }

    override fun onDestroy() {
        isDestroyed = true
        abandonPlaybackAudioFocus()
        unregisterNoisyReceiver()
        try {
            mediaSession?.isActive = false
            mediaSession?.release()
            mediaSession = null
        } catch (_: Exception) {}
        try {
            if (::webView.isInitialized) {
                webView.stopLoading()
                webView.removeJavascriptInterface("KrmziBridge")
                webView.loadUrl("about:blank")
                webView.clearHistory()
                webView.removeAllViews()
                webView.destroy()
            }
        } catch (e: Exception) {
            Log.w(TAG, "WebView cleanup failed: ${e.message}")
        }
        RustApi.clearMemoryCache()
        super.onDestroy()
    }
}
