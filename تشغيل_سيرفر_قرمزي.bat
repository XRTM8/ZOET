@echo off
chcp 65001 >nul
title خادم ZOET المحلي (قرمزي)
echo ====================================================
echo         تشغيل خادم ZOET المحلي لدعم قرمزي
echo ====================================================
echo.
echo الخادم يعمل الآن على الرابط: http://127.0.0.1:3000
echo يمكنك الآن فتح منصة ZOET Web أو تحديث الصفحة وستعمل قرمزي فوراً وبشكل تلقائي.
echo (اترك هذه النافذة مفتوحة طوال فترة المشاهدة)
echo.
set PORT=3000
set ZOET_PUBLIC=%~dp0public
if exist "%~dp0rust\otium-core\target\release\otium-server.exe" (
    "%~dp0rust\otium-core\target\release\otium-server.exe"
) else if exist "%~dp0..\ZOET\rust\otium-core\target\release\otium-server.exe" (
    "%~dp0..\ZOET\rust\otium-core\target\release\otium-server.exe"
) else (
    echo [خطأ] لم يتم العثور على ملف otium-server.exe
    pause
)
