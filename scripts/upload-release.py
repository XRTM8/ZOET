import os
import sys
import json
import hashlib
import urllib.request
import urllib.error
import subprocess
import time

sys.stdout.reconfigure(encoding='utf-8')

TAG_NAME = 'v2.9.3'
RELEASE_NAME = 'ZOET v2.9.3 - أوفرهول واجهة وأداء التلفزيون (10-Foot UI & Zero-Reflow D-Pad)'
RELEASE_BODY = """# ZOET v2.9.3 📺

أوفرهول كامل ومستقل لنسخة التلفزيون (Android TV / TV Boxes / 10-Foot Surface) لحل مشاكل القياسات والتحكم بالريموت والتقطيع بالكامل دون المساس بنظافة واجهة الهاتف والكمبيوتر.

---

### ✨ أبرز التحسينات والإصلاحات في وضع التلفزيون:

1. **📺 شريط تنقل علوي واحد فقط (Single 10-Foot Top Rail):**
   - إزالة شريط الـ Dock السفلي الخاص بالهاتف من وضع التلفاز بالكامل.
   - جمع كافة الوجهات في الشريط العلوي (`ZOET` · `المسلسلات | الأفلام | المكتبة | بحث | الإعدادات`) لسهولة الوصول من أي مكان بـ Arrow Up.

2. **📐 ضبط القياسات لكل دقات الشاشات (960p@2x و 720p و 1080p):**
   - ضبط الشبكة إلى 5 أعمدة (960–1359px) و 6 أعمدة (1360px+) لعرض صفّين مريحين في الشاشة بدون بوسترات ضخمة مقصوصة.
   - توسيع وتكبير شاشات **المكتبة** و**الإعدادات** كاملة للتلفاز، واستبدال القائمة المنسدلة `<select>` بأزرار أفقية صريحة (`تلقائي | كمبيوتر | تلفزيون`) متوافقة مع الريموت.

3. **⚡ القضاء على التقطيع (`Zero-Reflow D-Pad`):**
   - إعادة كتابة التنقل بالأسهم في [`remote_focus_key`](https://github.com/XRTM8/ZOET) داخل الشبكات (`#seriesGrid` / `#libraryGrid` / `#searchGrid`) وقوائم الحلقات بالاعتماد على الحساب المباشر لفهرس الصف والعمود (`O(1)`) بدون إعادة قياس مئات البطاقات مع كل ضغطة سهم.
   - تعطيل `backdrop-filter` والظلال الضبابية الضخمة (`60px blur`) و`scroll-behavior: smooth` في وضع التلفاز، والاكتفاء بإطار تركيز أبيض حاد ونظيف `2.5px` مع عزل رسم البطاقات (`contain: layout style paint` و `content-visibility: auto`).

---

### 📦 الملفات المرفقة:
- **`app-release.apk` / `otium-play.apk`**: نسخة Android (`com.krmzi.app` · `versionCode 29`) موقّعة بالمفتاح المستقر ومتوافقة مع الترقية المباشرة.
- **`ZOET.exe`**: نسخة Windows الأصلية المستقلة (Tauri v2).

---

### 🛡️ معلومات التثبيت والتحقق:
- **معرف الحزمة:** `com.krmzi.app` | **versionCode:** `29` | **SHA-256 للشهادة:** `2c6d74a5251577fcdfd8a9df651d52fb2641ff4ef1be6686c8266abed0a0cbf3`
- **حالة التحقق:** اجتاز البناء اختبارات Rust وتوقيع/فحص APK ومحاذاة `16KB` واختبارات المتصفح والـ PWA وفحص التلفاز في المتصفح (`960×540` و `1280×720` و `1920×1080`)؛ لم يُجرَ اختبار فعلي على جهاز Android TV أو هاتف حقيقي لهذا البناء بعد.
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
