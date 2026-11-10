import os
import sys
import json
import hashlib
import urllib.request
import urllib.error
import subprocess
import time

sys.stdout.reconfigure(encoding='utf-8')

TAG_NAME = 'v2.9.1'
RELEASE_NAME = 'ZOET v2.9.1 - واجهة AMOLED الزجاجية، التمرير اللانهائي، وإخفاء المصادر غير المتوفرة'
RELEASE_BODY = """# ZOET v2.9.1 🚀

تحديث شامل لتصميم وتجربة **ZOET** على الهاتف والكمبيوتر والمتصفح، مع إعادة بناء شريط التنقل وفصل الإعدادات والمكتبة، ومعالجة كاملة للتمرير اللانهائي وإدارة المصادر وتحديثات WASM.

---

### ✨ أبرز التحسينات والإصلاحات:

1. **🖤 إعادة تصميم AMOLED موحّد مع شريط عائم (Floating Smoked-Glass Dock):**
   - شريط تنقل زجاجي داكن عائم للتنقل السلس بين **القائمة** و**المكتبة** و**الإعدادات**، مع زر بحث دائري عائم بجانبه.
   - فصل صفحة **الإعدادات** في شاشة مستقلة تماماً عن **المكتبة**، وإخفاء تبويبات الكتالوج الفرعية خارج شاشات المسلسلات والأفلام.
   - استبدال الأيقونات السابقة بأيقونات SVG مدمجة وموحدة (صفر FontAwesome).

2. **♾️ تمرير لانهائي مستمر بلا حدود:**
   - تحميل تلقائي متواصل للصفحات التالية عند الوصول لنهاية القائمة في التصفح والبحث دون الحاجة لضغط زر متكرر.
   - إزالة حد الـ 480 عملاً السابق وإزالة عداد `24` المضلل من الترويسة.

3. **🌐 إدارة ذكية للمصادر وموثوقية التحديث:**
   - إبقاء **الكل** كخيار افتراضي لعرض كافة المصادر معاً.
   - إخفاء خيار **سينمانا** تلقائياً عندما لا تتوفر على اتصال الشبكة الحالي، وإعادته تلقائياً فور استعادة الاتصال.
   - إصلاح مشكلة تجمّد المكتبة/الإعدادات الناتجة عن كاش Service Worker القديم عبر مزامنة إصدار `v2.9.1` وسياسة Network-First لملفات التشغيل (`WASM/JS/CSS`).

---

### 📦 الملفات المرفقة:
- **`app-release.apk` / `otium-play.apk`**: نسخة Android (`com.krmzi.app` · `versionCode 27`) موقّعة بالمفتاح المستقر ومتوافقة مع التحديث المباشر داخل التطبيق.
- **`ZOET.exe`**: نسخة Windows الأصلية المستقلة (Tauri v2 مع المحرك والواجهة المدمجين).

---

### 🛡️ ملاحظات فنية ومعلومات التثبيت:
- **معرف الحزمة (Application ID):** `com.krmzi.app`
- **رقم الإصدار الداخلي (versionCode):** `27` (أعلى من `26` لترقية `v2.8.0` وما قبلها مباشرة دون حذف البيانات).
- **بصمة شهادة التوقيع (SHA-256):** `2c6d74a5251577fcdfd8a9df651d52fb2641ff4ef1be6686c8266abed0a0cbf3`
- **توفّر المصادر:** يعتمد الوصول المباشر إلى أفلام ومسلسلات **سينمانا** على الاتصال بشبكة إيرثلنك أو شبكة تدعمها؛ بينما يعمل **قرمزي** عبر كافة الشبكات (وفي المتصفح يتطلب قرمزي ربط خادم اتصال اختياري من الإعدادات).
- **حالة التحقق:** اجتاز البناء اختبارات Rust ومحاكاة المتصفح/PWA وحزمة APK الموقّعة؛ لم يُجرَ اختبار فعلي على جهاز هاتف Android أو شاشة TV حقيقية لهذا البناء بعد.
"""

def get_github_token():
    p = subprocess.Popen(['git', 'credential', 'fill'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    out, err = p.communicate('protocol=https\nhost=github.com\n\n')
    creds = dict(line.split('=', 1) for line in out.splitlines() if '=' in line)
    return creds.get('password')

def sha256_file(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(65536), b''):
            h.update(chunk)
    return h.hexdigest()

def ensure_release(headers):
    tag_url = f'https://api.github.com/repos/XRTM8/ZOET/releases/tags/{TAG_NAME}'
    check_req = urllib.request.Request(tag_url, headers=headers)
    try:
        with urllib.request.urlopen(check_req) as resp:
            release_info = json.loads(resp.read().decode())
            print(f"Existing release found: ID {release_info['id']} ({TAG_NAME})")
            # Keep title & body updated
            patch_data = json.dumps({
                'name': RELEASE_NAME,
                'body': RELEASE_BODY,
                'draft': False,
                'prerelease': False,
            }).encode('utf-8')
            patch_req = urllib.request.Request(
                f'https://api.github.com/repos/XRTM8/ZOET/releases/{release_info["id"]}',
                data=patch_data,
                headers={**headers, 'Content-Type': 'application/json'},
                method='PATCH',
            )
            with urllib.request.urlopen(patch_req) as presp:
                return json.loads(presp.read().decode())
    except urllib.error.HTTPError as e:
        if e.code != 404:
            raise
    print(f"Creating release {TAG_NAME} on XRTM8/ZOET (target main)...")
    create_data = json.dumps({
        'tag_name': TAG_NAME,
        'target_commitish': 'main',
        'name': RELEASE_NAME,
        'body': RELEASE_BODY,
        'draft': False,
        'prerelease': False,
    }).encode('utf-8')
    create_req = urllib.request.Request(
        'https://api.github.com/repos/XRTM8/ZOET/releases',
        data=create_data,
        headers={**headers, 'Content-Type': 'application/json'},
        method='POST',
    )
    with urllib.request.urlopen(create_req) as resp:
        return json.loads(resp.read().decode())

def main():
    token = get_github_token()
    if not token:
        print("ERROR: Could not get GitHub token from git credentials")
        sys.exit(1)

    headers = {
        'Authorization': 'token ' + token,
        'Accept': 'application/vnd.github.v3+json',
        'User-Agent': 'ZOET-Release-Uploader'
    }

    release_info = ensure_release(headers)
    print(f"Release ready: ID {release_info['id']}")

    upload_url_template = release_info['upload_url']
    upload_url_base = upload_url_template.split('{')[0]

    files_to_upload = [
        ('app-release.apk', 'release-output/app-release.apk', 'application/vnd.android.package-archive'),
        ('otium-play.apk', 'release-output/otium-play.apk', 'application/vnd.android.package-archive'),
        ('ZOET.exe', 'release-output/ZOET.exe', 'application/vnd.microsoft.portable-executable'),
    ]

    req = urllib.request.Request(f'https://api.github.com/repos/XRTM8/ZOET/releases/{release_info["id"]}', headers=headers)
    with urllib.request.urlopen(req) as resp:
        current_release = json.loads(resp.read().decode())
    existing_assets = {a['name']: a['id'] for a in current_release.get('assets', [])}

    for asset_name, asset_path, mime_type in files_to_upload:
        if not os.path.exists(asset_path):
            print(f"Skipping {asset_name} (file not found)")
            continue

        if asset_name in existing_assets:
            print(f"Asset {asset_name} already exists (ID: {existing_assets[asset_name]}), deleting first...")
            del_cmd = [
                'curl.exe', '--ssl-no-revoke', '-s', '-X', 'DELETE',
                '-H', f'Authorization: token {token}',
                f'https://api.github.com/repos/XRTM8/ZOET/releases/assets/{existing_assets[asset_name]}'
            ]
            subprocess.run(del_cmd, check=True)
            time.sleep(1)

        local_sha = sha256_file(asset_path)
        file_size = os.path.getsize(asset_path)
        print(f"\nUploading {asset_name} ({file_size} bytes, sha256:{local_sha})...")
        upload_url = f"{upload_url_base}?name={asset_name}"

        curl_cmd = [
            'curl.exe',
            '--ssl-no-revoke',
            '-X', 'POST',
            '-H', f'Authorization: token {token}',
            '-H', f'Content-Type: {mime_type}',
            '--retry', '5',
            '--retry-delay', '3',
            '--data-binary', f'@{os.path.abspath(asset_path)}',
            upload_url
        ]

        proc = subprocess.run(curl_cmd, capture_output=True, text=True)
        if proc.returncode != 0:
            print(f"ERROR: curl failed with code {proc.returncode}:\n{proc.stderr}")
            sys.exit(proc.returncode)

        resp_data = json.loads(proc.stdout)
        digest = resp_data.get('digest') or ''
        print(f"Uploaded {asset_name}: {resp_data.get('browser_download_url')} (remote digest: {digest})")
        if digest and digest != f"sha256:{local_sha}":
            print(f"ERROR: Digest mismatch for {asset_name}! expected sha256:{local_sha}, got {digest}")
            sys.exit(1)

    print(f"\n=== RELEASE {TAG_NAME} PUBLISHED AND ASSETS VERIFIED SUCCESSFULLY! ===")
    print(f"Release URL: {release_info['html_url']}")

if __name__ == '__main__':
    main()
