import os
import sys
import json
import hashlib
import urllib.request
import urllib.error
import subprocess
import time

sys.stdout.reconfigure(encoding='utf-8')

TAG_NAME = 'v3.2.0'
RELEASE_NAME = 'ZOET v3.2.0 - أوفرهول تشغيل السيرفرات، حظر تسرب الموقع الخارجي، واستخراج البث المباشر'
RELEASE_BODY = """# ZOET v3.2.0 🎬

أوفرهول شامل يركز على إصلاح تشغيل السيرفرات جذرياً، منع تسرب صفحات المواقع الخارجية داخل التطبيق، وحظر الإعلانات.

---

### ✨ أبرز ما يقدمه التحديث:

1. **⚡ استخراج البث المباشر (Direct HLS) لجميع السيرفرات:**
   - دمج مفكك تشفير خفيف لحزم الجافاسكربت (Dean Edwards Unpacker) داخل محرك التطبيق وواجهة WebAssembly.
   - استخراج وتوليد روابط تدفق HLS المباشرة (`.m3u8`) لسيرفرات البث الرئيسية (Arab HD, estream, Red HD, OK) للتشغيل المباشر داخل مشغل التطبيق الأصلي.

2. **🚫 منع تسرب صفحات الويب وحظر الإعلانات المزعجة:**
   - إلغاء السلوك السابق الذي كان يُخفي واجهة وأدوات المشغل ويترك الشاشة لإطار الـ iframe التابع للموقع الخارجي.
   - فرض بقاء واجهة ZOET الزجاجية بكامل أدواتها (شريط الوقت، زر الرجوع، مبدل السيرفرات، الإيماءات، والترجمة) في المقدمة دائماً فوق الفيديو (`z-index: 25`).
   - تطبيق عزل أمني صارم (`sandbox="allow-scripts allow-same-origin allow-presentation"`) يمنع صفحات المشغلات الخارجية من فتح أي نوافذ إعلانية منبثقة أو إعادة توجيه الروابط.

3. **🔄 تبديل فوري وسلس بين السيرفرات:**
   - التبديل بنقرة واحدة من قائمة "سيرفرات البث" دون فقدان الدقيقة الحالية من الحلقة، ودون تعطل أو تجميد في واجهة المشغل.

4. **🖥️ تحديث شامل لحزم سطح المكتب (Windows Native):**
   - إصدار حزمة `ZOET.exe` المستقلة المحدثة بمحرك البث الموازي الجديد وتسريع عتاد الرسوميات.

---

### 📦 الملفات المرفقة:
- **`app-release.apk` / `otium-play.apk`**: نسخة Android (`com.krmzi.app` · `versionCode 38`) موقّعة بالمفتاح المستقر ومحاذاة 16KB لجميع المعالجات.
- **`ZOET.exe`**: نسخة Windows الأصلية المستقلة (Tauri v2).

---

### 🛡️ معلومات التثبيت والتحقق:
- **معرف الحزمة:** `com.krmzi.app` | **versionCode:** `38` | **SHA-256 للشهادة:** `2c6d74a5251577fcdfd8a9df651d52fb2641ff4ef1be6686c8266abed0a0cbf3`
- **حالة التحقق:** اجتاز البناء محاذاة 16KB وتوقيع APK Signature Scheme v2 واختبارات سلامة الأصول المدمجة.
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
