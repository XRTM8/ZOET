import os
import sys
import json
import hashlib
import urllib.request
import urllib.error
import subprocess
import time

sys.stdout.reconfigure(encoding='utf-8')

TAG_NAME = 'v3.3.0'
RELEASE_NAME = 'ZOET v3.3.0 - التحديث الشامل: أوفر هول الشاشات والتلفزيون (Leanback TV)، والواجهة البريميوم، واستقرار السيرفرات'
RELEASE_BODY = """# ZOET v3.3.0 🎬

أوفرهول وتحديث شامل يعيد بناء تجربة شاشات وأجهزة التلفزيون (Android TV / Leanback)، بالإضافة للواجهة البريميوم والأنيميشنات التفاعلية، تنظيم بوسترات المسلسلات والأفلام بدقة عالية، وتثبيت بث السيرفرات المباشر.

---

### 📺 1. أوفرهول كامل لتجربة التلفزيون والشاشات الكبيرة (Android TV & Leanback):
- **تحكم فوري ودقيق بالريموت (Zero-Lag D-Pad Navigation):**
  - إلغاء عمليات إعادة حساب أبعاد الشاشة المتكررة (`getComputedStyle`) عند كل ضغطة ريموت، والانتقال لمحاذاة إحداثية مباشرة ومصفوفة سريعة في Rust WASM.
  - انتقال حتمي ومريح بين الأقسام: الضغط على السهم لأعلى من الصف الأول ينتقل مباشرة لتبويبات (الأفلام / المسلسلات)، والنزول لأسفل يعود مباشرة للبطاقة الأولى دون تيه.
  - تنقل انسيابي داخل تفاصيل المسلسلات بين زر "متابعة المشاهدة"، تبويبات المواسم، وبطاقات الحلقات بسلاسة تامة.
- **واجهة 10-Foot سينمائية واضحة ومريحة للعين:**
  - تعديل شبكة الأعمال لتتسع لـ 5 إلى 6 بوسترات كبيرة في الصف الواحد بدلاً من تصغيرها وحشر 10 بوسترات.
  - إطار تركيز (Focus Indicator) أبيض بارز ومضيء (`scale: 1.09`) مع ظلال ثلاثية الأبعاد تمكنك من معرفة مكان مؤشر الريموت من مسافة أمتار.
  - تكبير خطوط العناوين والتقييمات والتفاصيل لتكون مقروءة وواضحة جداً على شاشات 1080p و 4K.
- **تحسينات مشغل الفيديو للريموت:**
  - ظهور مؤشر رقمي فوري (`+10 ثوانٍ` / `-10 ثوانٍ`) وشريط التقديم تلقائياً عند النقر على السهمين يميناً ويساراً في الريموت لتعرف أين وصلت قبل الإخفاء التلقائي.
  - زر الرجوع (Back Key) الذكي: إخفاء واجهة التحكم أولاً إذا كانت ظاهرة، أو الخروج من المشغل دون فقدان مكانك في القائمة الرئيسية.
- **تسريع العتاد لشاشات التلفزيون (Hardware Layer Acceleration):**
  - تفعيل تسريع المعالج الرسومي (GPU Layer) وضبط هوية الشاشات الكبيرة لضمان سلاسة التنقل والانسيابية التامة.

---

### ✨ 2. تجربة بصرية وحركية راقية (Premium Tactile Experience):
- **أنيميشن تحميل فاخر (Luxury Shimmer Gradient):** بدلاً من الهياكل الرمادية السابقة، مع ضبط أبعاد كروت التحميل إلى نسبة (2:3) لمنع قفز وتداخل العناصر.
- **تبويبات كبسولية زجاجية تفاعلية (Tactile Glass Capsule Tabs):** لتبويب الأفلام والمسلسلات مع استجابة فيزيائية ناعمة للمس (`scale: 0.95`).
- **تحسين القوائم السفلية المنبثقة (Bottom Sheets):** بأنيميشن سحب نابض (iOS Spring Slide-up)، تدرجات زجاجية معتمة بدقة، ومقبض سحب متناسق.
- **إعادة تصميم بطاقات الحلقات وتبويبات المواسم:** لتصبح كبسولات زجاجية أنيقة ذات استجابة لمسية ناعمة.

---

### 🖼️ 3. تنظيم وعرض بوسترات المسلسلات والأفلام بأعلى دقة:
- **اعتماد البوسترات الأصلية (Full HD Artwork) كأولوية قصوى.**
- **إضافة تدرج ظلي متوازن لقراءة العناوين بوضوح تام،** مع إبراز تقييم العمل والنوع وحالة الترجمة والدبلجة.
- **حذف مصادر قرمزي العالقة وحصر المصادر بكتالوج سينمانا المباشر والمستقر.**

---

### ⚡ 4. استخراج البث المباشر (Direct HLS) لجميع السيرفرات:
- استخراج وتوليد روابط تدفق HLS المباشرة (`.m3u8`) لسيرفرات البث الرئيسية (Arab HD, estream, Red HD, OK) للتشغيل المباشر داخل مشغل التطبيق الأصلي دون إعلانات منبثقة أو خروج من المشغل.

---

### 💻 5. أجهزة الكمبيوتر وسطح المكتب (Windows / Mac / Linux):
- اعتماد منصة الويب الرسمية فائقة السرعة المعتمدة على محرك Rust WebAssembly: **[https://xrtm8.github.io/ZOET-Web/](https://xrtm8.github.io/ZOET-Web/)**.
- **التثبيت كتطبيق مستقل (PWA)**: افتح الرابط في متصفحك (Edge أو Chrome أو Brave) ثم اضغط على أيقونة **"تثبيت التطبيق" (Install App)** في شريط العناوين لتحصل على التطبيق فوراً على سطح مكتبك وقائمة ابدأ بدون الحاجة لتحميل ملفات exe أو مشاكل الحظر والتثبيت.

---

### 📦 الملفات المرفقة:
- **`app-release.apk` / `otium-play.apk`**: نسخة Android الرسمية للهواتف والتلفزيون (`com.krmzi.app` · `versionCode 39`) موقّعة بالمفتاح المستقر ومحاذاة 16KB لجميع المعالجات.
- **نسخة الويب والحاسوب**: متاحة ومثبتة مباشرة عبر الرابط [https://xrtm8.github.io/ZOET-Web/](https://xrtm8.github.io/ZOET-Web/)

---

### 🛡️ معلومات التثبيت والتحقق:
- **معرف الحزمة:** `com.krmzi.app` | **versionCode:** `39` | **SHA-256 للشهادة:** `2c6d74a5251577fcdfd8a9df651d52fb2641ff4ef1be6686c8266abed0a0cbf3`
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
