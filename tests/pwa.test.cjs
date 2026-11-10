'use strict';
const assert=require('node:assert/strict');
const {chromium}=require(process.env.PLAYWRIGHT_MODULE||'C:/Users/mjd20/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const {server,failures}=require('./rust-ui.test.cjs');
async function run(){
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const browser=await chromium.launch({executablePath:process.env.BROWSER_PATH||'C:/Users/mjd20/AppData/Local/BraveSoftware/Brave-Browser/Application/brave.exe',headless:true});
  try{
    const context=await browser.newContext({viewport:{width:390,height:844}});
    const page=await context.newPage();page.on('pageerror',error=>failures.push(error.message));
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.waitForFunction(()=>!!navigator.serviceWorker.controller,null,{timeout:15000});
    const cached=await page.evaluate(async()=>{const names=await caches.keys();const shell=names.find(n=>n.startsWith('zoet-shell-'))||'zoet-shell-2.9.5';const cache=await caches.open(shell);return (await cache.keys()).map(request=>new URL(request.url).pathname);});
    assert(cached.includes('/wasm/otium_ui_bg.wasm'));
    assert(cached.includes('/wasm/otium_ui.js'));
    assert(!cached.some(path=>path.startsWith('/api/')),'No stale signed stream/API data in browser platform cache');
    await context.setOffline(true);await page.reload();
    await page.waitForFunction(()=>document.getElementById('bootStatus').classList.contains('hidden'));
    assert(await page.locator('#tabSeries').isVisible());assert(await page.locator('#tabMovies').isVisible());
    assert.deepEqual(failures,[]);
    console.log('PASS Rust PWA: atomic generated-WASM shell cache, offline boot, no API/stream caching');
    await context.close();
  }finally{await browser.close();await new Promise(resolve=>server.close(resolve));}
}
run().catch(error=>{console.error(error);server.close();process.exitCode=1;});
