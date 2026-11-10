/* Browser platform cache. Catalogue and playback logic run in Rust/WASM. */
'use strict';
const CACHE='zoet-shell-2.9.5';
const SHELL=['./','./index.html','./styles.css?v=2.9.5','./manifest.json','./wasm/otium_ui.js?v=2.9.5','./wasm/otium_ui_bg.wasm?v=2.9.5','./vendor/hls/hls.min.js','./fonts/tajawal.css','./icons/icon-192.png','./icons/icon-512.png',...Array.from({length:10},(_,i)=>`./fonts/tajawal-${i+1}.woff2`)];
self.addEventListener('install',event=>event.waitUntil(caches.open(CACHE).then(cache=>cache.addAll(SHELL)).then(()=>self.skipWaiting())));
self.addEventListener('activate',event=>event.waitUntil((async()=>{
  const names=await caches.keys();
  await Promise.all(names.filter(name=>(name.startsWith('krmzi-')||name.startsWith('otium-shell-')||name.startsWith('zoet-shell-'))&&name!==CACHE).map(name=>caches.delete(name)));
  await self.clients.claim();
})()));
self.addEventListener('fetch',event=>{
  const request=event.request,url=new URL(request.url);
  if(request.method!=='GET'||url.origin!==self.location.origin||url.pathname.startsWith('/api/'))return;
  if(request.mode==='navigate'){
    event.respondWith(fetch(request).catch(()=>caches.open(CACHE).then(cache=>cache.match('./index.html'))));return;
  }
  if(/\.(css|js|wasm)$/.test(url.pathname)){
    event.respondWith(caches.open(CACHE).then(async cache=>{
      try{
        const response=await fetch(request);
        if(response&&response.ok)await cache.put(request,response.clone());
        return response;
      }catch{
        return (await cache.match(request))||(await cache.match(url.pathname));
      }
    }));
    return;
  }
  if(/\.(woff2|png|svg|json)$/.test(url.pathname)){
    event.respondWith(caches.open(CACHE).then(async cache=>{
      const saved=await cache.match(request);if(saved)return saved;
      const response=await fetch(request);if(response.ok)await cache.put(request,response.clone());return response;
    }));
  }
});
