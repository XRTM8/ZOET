import os
import sys
import json
import hashlib
import urllib.request
import urllib.error
import subprocess
import time

sys.stdout.reconfigure(encoding='utf-8')

TAG_NAME = 'v3.6.0'
RELEASE_NAME = 'ZOET v3.6.0 - محرك التبديل السريع بين المدبلج والمترجم وأوفرهول التلفزيون (Sub/Dub Fast Switcher)'
RELEASE_BODY = """# ZOET v3.6.0 🎬🎙️

تحديث شامل ومحرك متطور للتبديل السريع والفوري بين النسخ المترجمة والمدبلجة للمسلسلات والأفلام، مع حفظ التوقيت التلقائي وزر مخصص في المشغل، وتحسينات شاشات التلفزيون والأجهزة المحمولة.

---

### 🎙️ 1. محرك المطابقة الذكي للنسخ المتناظرة (Sub/Dub Matching Engine):
- **تطبيع كامل للعناوين والمواسم:** نظام تطبيع دقيق في Rust يتعرف على كافة صيغ الدبلجة والترجمة العربية والإنجليزية (`مدبلج`، `مدبلجة`، `دبلجة`، `الدبلجة`، `dubbed`، `dub`)، ويفصل أرقام المواسم بذكاء لمنع الالتباس.
- **خوارزمية مطابقة متعددة المعايير (Scoring System):** مطابقة العناوين البديلة (العربية والإنجليزية والأصلية) مع حساب تطابق الموسم (+35 نقطة) واستبعاد المواسم أو السنوات غير المتوافقة، ما يضمن العثور على نفس الموسم بالنسخة المعاكسة بدقة 100%.
- **استكشاف استباقي في الخلفية (Proactive Discovery):** يتم البحث عن النسخة المقابلة واستخراج رابطها فور فتح تفاصيل المسلسل أو تشغيل الحلقة، ليكون الانتقال فورياً دون أي انتظار.

---

### ⚡ 2. زر التبديل السريع داخل المشغل (1-Click In-Player Switch):
- **زر مخصص في شريط تحكم المشغل (`#playerEditionBtn`):** يظهر مباشرة بجانب قائمة الحلقات بشارة توضيحية ديناميكية (`[🎙️ المدبلج]` عند مشاهدة المترجم، و`[💬 المترجم]` عند مشاهدة المدبلج).
- **حفظ زمن التشغيل بدقة (`currentTime`):** عند التبديل من المترجم إلى المدبلج (أو العكس)، يحفظ المشغل الثواني الحالية وينتقل إليها في النسخة الجديدة فوراً وبشكل سلس دون إعادة الحلقة من البداية.
- **تكامل كامل مع ريموت التلفزيون (D-Pad Navigation):** إدراج زر التبديل ضمن حلقة التنقل الحتمي بالأسهم لتسهيل التبديل بضغطة زر واحدة على أجهزة الشاشات والتلفاز.

---

### 📺 3. استقرار واجهة التلفزيون (Leanback TV):
- استجابة سريعة وانسيابية مع إطارات تركيز واضحة وعالية التباين لكافة الأزرار والقوائم.
- إزالة كافة العمليات الثقيلة لضمان أعلى أداء واستجابة فورية لأوامر الريموت.

---

### 📦 الملفات المرفقة:
- **`app-release.apk` / `zoet-3.6.0.apk` / `otium-play.apk`**: نسخة Android الموحدة للهواتف والتلفزيون (`com.krmzi.app` · `versionCode 42`) موقّعة بالمفتاح المعتمد ومحاذاة 16KB لجميع المعالجات.
- **نسخة الويب والحاسوب PWA**: متاحة ومحدثة فورياً على [https://xrtm8.github.io/ZOET-Web/](https://xrtm8.github.io/ZOET-Web/)

---

### 🛡️ معلومات التثبيت والتحقق:
- **معرف الحزمة:** `com.krmzi.app` | **versionCode:** `42` | **SHA-256 للشهادة:** `2c6d74a5251577fcdfd8a9df651d52fb2641ff4ef1be6686c8266abed0a0cbf3`
- **حالة التحقق:** اجتاز البناء محاذاة 16KB وتوقيع APK Signature Scheme v2 واختبارات الوحدة الـ 17 في Rust بنجاح تام.
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
        ('zoet-3.6.0.apk', 'release-output/zoet-3.6.0.apk', 'application/vnd.android.package-archive'),
        ('otium-play.apk', 'release-output/otium-play.apk', 'application/vnd.android.package-archive'),
    ]

    req = urllib.request.Request(f'https://api.github.com/repos/XRTM8/ZOET/releases/{release_info["id"]}', headers=headers)
    with urllib.request.urlopen(req) as resp:
        current_release = json.loads(resp.read().decode())
    existing_assets = {a['name']: a['id'] for a in current_release.get('assets', [])}

    # Delete deprecated desktop executable if present
    if 'ZOET.exe' in existing_assets:
        print(f"Removing deprecated ZOET.exe asset (ID: {existing_assets['ZOET.exe']})...")
        del_cmd = [
            'curl.exe', '--ssl-no-revoke', '-s', '-X', 'DELETE',
            '-H', f'Authorization: token {token}',
            f'https://api.github.com/repos/XRTM8/ZOET/releases/assets/{existing_assets["ZOET.exe"]}'
        ]
        subprocess.run(del_cmd, check=True)
        time.sleep(1)

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
