'use strict';

// Exercise the shipped Rust/WASM interface with deterministic upstream pages.
// Video events are mocked; WebAssembly, DOM, network and lifecycle code are real.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const http = require('node:http');
const path = require('node:path');
const {chromium} = require(process.env.PLAYWRIGHT_MODULE || 'C:/Users/mjd20/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');

const publicDir = path.resolve(__dirname, '../public');
const seriesUrl = 'https://krmzi.org/series/test-series/';
const episodeUrl = number => `https://krmzi.org/episode/test-${number}/`;
const artwork = '/test-images/series.png';
const episodeArtwork = '/test-images/episode.png';
const series = {url:seriesUrl,title:'مسلسل التجربة',thumbnail:artwork,type:'series',source:'krmzi'};
const krSeries = [series,...['الثاني','الثالث','الرابع'].map((name,index)=>({...series,url:`https://krmzi.org/series/other-${index+1}/`,title:`مسلسل ${name}`}))];
const episodes = Array.from({length:80},(_,index)=>({url:episodeUrl(index+1),title:`مسلسل التجربة الحلقة ${index+1}`,seriesTitle:series.title,episodeNum:String(index+1),season:'1',seriesUrl,thumbnail:artwork}));
const cinema = (id,title='التجربة',kind='2')=>({nb:String(id),ar_title:title,en_title:title,kind,season:'1',episodeNummer:'1',rootSeries:'0',imgObjUrl:artwork,imgMediumThumbObjUrl:artwork,year:'2025'});
const cinemaSeries = [cinema(700),...Array.from({length:23},(_,index)=>cinema(900+index,`حكاية سينمانا ${index+1}`))];
const requests = [];
const failures = [];
let failCinemana = false;
let failEpisode = false;
let delayEpisode = 0;
let delayCinemana = 0;
let cinemanaHls = false;

const server = http.createServer(async(req,res)=>{
  const url = new URL(req.url,'http://localhost');
  requests.push(url.pathname+url.search);
  res.setHeader('Cache-Control','no-store');
  if (url.pathname.startsWith('/api/')) {
    res.setHeader('Content-Type','application/json; charset=utf-8');
    const page = Number(url.searchParams.get('page')||1);
    let data;
    switch(url.pathname) {
      case '/api/series': data={page,series:page===1?krSeries.slice(0,2):krSeries.slice(2),hasNext:true,nextPage:page+1};break;
      case '/api/episodes': data={page,episodes,hasNext:false,nextPage:null};break;
      case '/api/search': {
        const q=(url.searchParams.get('q')||'').trim();
        data=q==='لايوجد_اطلاقا'?{page,results:[],hasNext:false,nextPage:null}:{page,results:page===1?[series]:[krSeries[2]],hasNext:page===1,nextPage:page===1?2:null};
        break;
      }
      case '/api/series/detail': data={...series,url:url.searchParams.get('url'),poster:artwork,story:'قصة قصيرة للاختبار',episodes};break;
      case '/api/episode/detail': {
        if(delayEpisode)await new Promise(resolve=>setTimeout(resolve,delayEpisode));
        if(failEpisode){res.statusCode=502;data={error:'Provider unavailable'};break;}
        const episode = episodes.find(item=>item.url===url.searchParams.get('url'))||episodes[0];
        data={...episode,poster:episodeArtwork,directStreams:[{quality:'1080p (Full HD)',url:'/test-stream/full.mp4'},{quality:'720p (HD)',url:'/test-stream/hd.mp4'},{quality:'480p (SD)',url:'/test-stream/sd.mp4'}],
          servers:[{id:'mirror-1',name:'OK',embedUrl:'/test-embed/ok'},...Array.from({length:12},(_,index)=>({name:`سيرفر تجريبي باسم طويل جداً ${index+1}`,embedUrl:`/test-embed/${index+1}`}))]};if(url.searchParams.has('server'))data.directStreams=data.directStreams.map(stream=>({...stream,url:stream.url.replace('.mp4','-mirror.mp4')}));break;
      }
      case '/api/cinemana': {
        if(delayCinemana)await new Promise(resolve=>setTimeout(resolve,delayCinemana));
        if(failCinemana){res.statusCode=503;data={error:'cinemana_unavailable'};break;}
        const route=url.searchParams.get('path');
        if(route.startsWith('latestSeries/')) data=route.includes('/page/0/')?cinemaSeries:[cinema(950,'مسلسل إضافي من سينمانا')];
        else if(route.startsWith('latestMovies/'))data=[cinema(800,'فيلم سينمانا الأول','1'),cinema(801,'فيلم سينمانا الثاني','1')];
        else if(route.startsWith('video/V/')){
          const word=route.split('/video_title_search/')[1]?.split('/')[0]||'';
          const decoded=Buffer.from(word,'base64url').toString('utf8');
          if(decoded==='The Batman'){
            data=[cinema(881,'Batman','1'),{...cinema(880,'The Batman','1'),year:'2022',stars:'7.8'}];
          }else if(decoded==='اوبنهايمر'||decoded==='لايوجد_اطلاقا'){
            data=[];
          }else if(decoded==='Oppenheimer'){
            data=[{...cinema(882,'Oppenheimer','1'),year:'2023',stars:'8.4'}];
          }else{
            data=route.includes('/pageNumber/0/')?cinemaSeries.slice(0,12):[cinema(960,'نتيجة إضافية')];
          }
        }
        else if(route.startsWith('videoSeason/'))data=[cinema(700),{...cinema(701),episodeNummer:'2',rootSeries:'700'},{...cinema(702),season:'2',rootSeries:'700'}];
        else if(route.startsWith('allVideoInfo/')){
          const id=route.split('/').at(-1);data=id==='800'||id==='801'?cinema(id,`فيلم سينمانا ${id}`,'1'):{...cinema(id),episodeNummer:id==='701'?'2':'1',season:id==='702'?'2':'1',rootSeries:id==='700'?'0':'700'};
        }
        else if(route.startsWith('transcoddedFiles/'))data=[{resolution:'720p',container:cinemanaHls?'m3u8':'mp4',videoUrl:cinemanaHls?'https://cdn.shabakaty.com/master.m3u8':'https://cdn.shabakaty.com/test-720.mp4'}];
        else data=[];
        break;
      }
      default: data={ok:true};
    }
    res.end(JSON.stringify(data));return;
  }
  if(url.pathname.startsWith('/test-images/')){
    res.setHeader('Content-Type','image/png');res.end(Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j4z0AAAAASUVORK5CYII=','base64'));return;
  }
  if(url.pathname.startsWith('/test-stream/')){res.statusCode=204;res.end();return;}
  if(url.pathname.startsWith('/test-embed/')){res.setHeader('Content-Type','text/html');res.end('<!doctype html><title>Test provider</title>');return;}
  const file=path.resolve(publicDir,'.'+(url.pathname==='/'?'/index.html':url.pathname));
  if(!file.startsWith(publicDir+path.sep)){res.statusCode=403;res.end();return;}
  try {
    res.setHeader('Content-Type',({'.html':'text/html','.css':'text/css','.js':'text/javascript','.wasm':'application/wasm','.json':'application/json','.woff2':'font/woff2'})[path.extname(file)]||'application/octet-stream');
    res.end(fs.readFileSync(file));
  }catch{res.statusCode=404;res.end();}
});

function mediaMock(source='all') {
  try{localStorage.setItem('otium_source',source);}catch{}
  Object.defineProperty(navigator,'connection',{configurable:true,value:{effectiveType:'4g',saveData:false}});
  window.__media={time:0,duration:1800,paused:true,readyState:0,bufferedEnd:0,loads:[],plays:0};
  const p=HTMLMediaElement.prototype,getAttribute=p.getAttribute,removeAttribute=p.removeAttribute;
  Object.defineProperty(p,'src',{configurable:true,get(){return this.__testSource||'';},set(value){this.__testSource=value;}});
  p.getAttribute=function(name){return name==='src'?this.__testSource||null:getAttribute.call(this,name);};
  p.removeAttribute=function(name){if(name==='src')this.__testSource='';else removeAttribute.call(this,name);};
  for(const [key,field]of[['currentTime','time'],['duration','duration'],['paused','paused'],['readyState','readyState']])Object.defineProperty(p,key,{configurable:true,get(){return window.__media[field];},set(value){window.__media[field]=value;}});
  Object.defineProperty(p,'buffered',{configurable:true,get(){return{length:window.__media.bufferedEnd>0?1:0,start(){return 0;},end(){return window.__media.bufferedEnd;}};}});
  p.load=function(){window.__media.time=0;window.__media.readyState=0;window.__media.loads.push(this.getAttribute('src'));};
  p.pause=function(){window.__media.paused=true;this.dispatchEvent(new Event('pause'));};
  p.play=function(){window.__media.paused=false;window.__media.plays++;this.dispatchEvent(new Event('play'));return Promise.resolve();};
  window.__ready=()=>{window.__media.readyState=4;const video=document.getElementById('nativePlayer');for(const event of ['loadedmetadata','canplay','playing'])video.dispatchEvent(new Event(event));};
}

async function run(){
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const origin=`http://127.0.0.1:${server.address().port}`;
  const executablePath=process.env.BROWSER_PATH||['C:/Program Files/BraveSoftware/Brave-Browser/Application/brave.exe','C:/Users/mjd20/AppData/Local/BraveSoftware/Brave-Browser/Application/brave.exe',chromium.executablePath()].find(file=>fs.existsSync(file));
  const browser=await chromium.launch({executablePath,headless:true,args:['--autoplay-policy=no-user-gesture-required']});
  async function open(source='all',viewport={width:390,height:844},clock=false){
    const context=await browser.newContext({viewport,serviceWorkers:'block'});await context.addInitScript(mediaMock,source);await context.route('https://**/*',route=>route.abort());
    const page=await context.newPage();page.on('pageerror',error=>failures.push(error.message));if(clock)await page.clock.install();await page.goto(origin);await page.locator('#seriesGrid [data-action="series"]').first().waitFor();return{context,page};
  }
  try{
    {
      const {context,page}=await open();
      await page.waitForFunction(()=>document.querySelectorAll('#seriesGrid [data-action="series"]').length===25);
      assert.equal(await page.locator('#heroSpotlight,#episodesCarousel,#moviesCarousel').count(),0,'The interface should contain one catalogue, without promotional carousel rows');
      assert.equal(await page.locator('#seriesGrid [data-url="cinemana:700"] .provider-tag').count(),2,'Shared titles retain both sources');
      await page.evaluate(()=>{const c=document.getElementById('contentCanvas');c.scrollTop=c.scrollHeight;});await page.waitForFunction(()=>document.querySelectorAll('#seriesGrid [data-action="series"]').length===28);
      await page.evaluate(()=>{const c=document.getElementById('contentCanvas');c.scrollTop=c.scrollHeight;const b=document.getElementById('loadMoreSeriesBtn');if(b&&!b.classList.contains('hidden')&&!b.disabled)b.click();});await page.waitForFunction(()=>document.getElementById('loadMoreSeriesBtn').classList.contains('hidden'));
      assert.equal(await page.locator('#seriesGrid .series-tile').count(),28,'Repeated upstream pages stop without duplicate cards');
      assert(!requests.some(request=>request==='/api/series?page=4'),'Repeated pages do not continue forever');
      await page.locator('#tabMovies').click();await page.waitForFunction(()=>document.querySelectorAll('#seriesGrid [data-action="episode"]').length===2);
      assert((await page.locator('#seriesGrid').textContent()).includes('فيلم سينمانا الأول'));
      assert(!(await page.locator('#seriesGrid').textContent()).includes('مسلسل التجربة'),'Movies and series use separate views');
      await page.locator('#tabSeries').click();await page.waitForFunction(()=>document.querySelectorAll('#seriesGrid [data-action="series"]').length===25);
      await page.locator('#openSearchBtn').click();await page.locator('#overlaySearchInput').fill('التجربة');
      await page.waitForFunction(()=>document.querySelectorAll('#searchGrid .series-tile').length===12);
      assert(requests.filter(request=>request.startsWith('/api/cinemana?')).some(request=>{const route=new URL(request,'http://fixture').searchParams.get('path');const word=route.split('/video_title_search/')[1]?.split('/')[0];return word&&Buffer.from(word,'base64url').toString('utf8')==='التجربة';}),'Cinemana search sends UTF-8 Base64URL as required by its API');
      await page.evaluate(()=>{const s=document.getElementById('searchScrollArea');s.scrollTop=s.scrollHeight;});await page.waitForFunction(()=>document.querySelectorAll('#searchGrid .series-tile').length===14);
      assert(await page.locator('#loadMoreSearchBtn').evaluate(el=>el.classList.contains('hidden')));
      await page.keyboard.press('Escape');assert(await page.locator('#searchOverlay').evaluate(el=>el.classList.contains('hidden')));
      await context.close();console.log('PASS Rust catalogue: complete pages, duplicate guard, provider merge, films and paginated search');
    }
    {
      failCinemana=true;
      const{context,page}=await open();await page.waitForFunction(()=>document.getElementById('cataloguePageStatus').textContent.includes('سينمانا'));
      assert.equal(await page.locator('#seriesGrid .series-tile').count(),2,'A provider failure preserves the other catalogue');
      failCinemana=false;await page.locator('#loadMoreSeriesBtn').click();await page.waitForFunction(()=>document.querySelectorAll('#seriesGrid .series-tile').length>2);
      await page.waitForFunction(()=>!document.getElementById('cataloguePageStatus').textContent.includes('تعذّر'));
      await context.close();console.log('PASS Rust catalogue: partial provider failure and retry');
    }
    for(const viewport of [{width:320,height:640},{width:390,height:844},{width:844,height:390},{width:1280,height:800}]){
      const{context,page}=await open('krmzi',viewport);
      const bounds=await page.evaluate(()=>({width:innerWidth,scroll:document.documentElement.scrollWidth,frame:document.getElementById('appFrame').getBoundingClientRect().width}));
      assert(bounds.scroll<=bounds.width+1,`Page overflow at ${viewport.width}`);assert(bounds.frame>=250&&bounds.frame<=bounds.width+1);
      await page.locator('#seriesGrid [data-url="'+seriesUrl+'"]').click();await page.locator('.sheet-episode-btn').last().waitFor();
      assert.equal(await page.locator('.sheet-episode-btn').count(),80);
      const heights=await page.locator('.sheet-episode-btn').evaluateAll(rows=>rows.map(row=>row.getBoundingClientRect().height));assert(heights.every(height=>height>=44),`Episode rows are too small at ${viewport.width}`);
      await page.locator('.sheet-episode-btn').last().scrollIntoViewIfNeeded();assert(await page.locator('.sheet-episode-btn').last().isVisible());
      await page.locator('#sheetEpisodeSearch').fill('80');assert.equal(await page.locator('.sheet-episode-btn').count(),1);await page.locator('#sheetEpisodeSearch').fill('');
      const before=requests.length;await page.locator('.sheet-episode-btn').first().click();
      await page.waitForFunction(()=>document.getElementById('nativePlayer').src.includes('/test-stream/hd.mp4'));
      assert.equal(await page.locator('#nativePlayer').getAttribute('poster'),null);assert(!requests.slice(before).some(request=>request===episodeArtwork),'Playback does not fetch episode art');
      assert((await page.locator('#playerBackdrop').evaluate(el=>el.style.backgroundImage)).includes('series.png'));
      await page.evaluate(()=>window.__ready());await page.locator('#openServersSheetBtn').click();
      await page.waitForTimeout(200);const panel=await page.locator('#bottomSheet').boundingBox();assert(panel.y>=-1&&panel.y+panel.height<=viewport.height+1,`Menu outside viewport at ${viewport.width}`);
      await page.locator('#closeSheetBtn').click();await page.locator('#dismissPlayerBtn').click();
      assert(await page.locator('#mobilePlayer').evaluate(el=>el.classList.contains('hidden')));assert.equal(await page.locator('#nativePlayer').getAttribute('src'),null);
      await context.close();console.log(`PASS Rust viewport ${viewport.width}x${viewport.height}: episode rows, media start, artwork, menus and close`);
    }
    {
      const{context,page}=await open('all');
      await page.locator('#seriesGrid [data-url="cinemana:700"]').click();
      await page.locator('[data-series-source="'+seriesUrl+'"]').click();
      await page.locator('.sheet-episode-btn').first().click();
      await page.waitForFunction(()=>document.getElementById('nativePlayer').src.endsWith('/hd.mp4'));
      await page.evaluate(()=>{window.__ready();const v=document.getElementById('nativePlayer');window.__media.time=137;window.__media.paused=true;v.playbackRate=1.5;v.volume=.4;v.muted=true;});
      delayCinemana=150;
      await page.locator('#openServersSheetBtn').click();await page.locator('[data-player-action="provider"][data-url="cinemana:700"]').click();
      await page.waitForTimeout(60);
      assert((await page.locator('#nativePlayer').evaluate(v=>v.src)).endsWith('/hd.mp4'),'Alternate metadata loads without interrupting active media');
      await page.waitForFunction(()=>document.getElementById('nativePlayer').src.includes('test-720.mp4'));
      await page.evaluate(()=>{const v=document.getElementById('nativePlayer');v.dispatchEvent(new Event('loadedmetadata'));v.dispatchEvent(new Event('canplay'));});
      assert.deepEqual(await page.locator('#nativePlayer').evaluate(v=>({time:v.currentTime,paused:v.paused,rate:v.playbackRate,volume:v.volume,muted:v.muted})),{time:137,paused:true,rate:1.5,volume:.4,muted:true},'Cross-provider switch preserves all playback settings');
      delayCinemana=0;failEpisode=true;
      await page.locator('#openServersSheetBtn').click();await page.locator('[data-player-action="provider"][data-url="'+seriesUrl+'"]').click();
      await page.waitForFunction(()=>document.getElementById('sheetTitle').textContent.includes('غير متاح'));
      assert((await page.locator('#nativePlayer').evaluate(v=>v.src)).includes('test-720.mp4'),'Unavailable replacement does not stop the current source');
      assert.equal(await page.locator('#nativePlayer').evaluate(v=>v.currentTime),137);
      failEpisode=false;await page.locator('#closeSheetBtn').click();
      // Failed media after successful metadata must roll back to the previous source.
      await page.locator('#openServersSheetBtn').click();await page.locator('[data-player-action="provider"][data-url="'+seriesUrl+'"]').click();
      await page.waitForFunction(()=>document.getElementById('nativePlayer').src.endsWith('/hd.mp4'));
      for(const ending of ['/sd.mp4','/full.mp4','test-720.mp4']){
        await page.evaluate(()=>document.getElementById('nativePlayer').dispatchEvent(new Event('error')));
        await page.waitForFunction(ending=>document.getElementById('nativePlayer').src.endsWith(ending),ending);
      }
      await page.evaluate(()=>{const v=document.getElementById('nativePlayer');v.dispatchEvent(new Event('loadedmetadata'));v.dispatchEvent(new Event('canplay'));});
      assert.equal(await page.locator('#nativePlayer').evaluate(v=>v.currentTime),137,'Rollback restores playback position');
      // Cancellation while resolving the other provider may not replace the current source later.
      delayEpisode=500;await page.locator('#openServersSheetBtn').click();await page.locator('[data-player-action="provider"][data-url="'+seriesUrl+'"]').click();
      await page.keyboard.press('Escape');await page.waitForTimeout(650);
      assert((await page.locator('#nativePlayer').evaluate(v=>v.src)).includes('test-720.mp4'),'Closing source sheet cancels a pending replacement');
      delayEpisode=0;
      await page.locator('#playerCanvas').focus();await page.keyboard.press('l');assert.equal(await page.locator('#nativePlayer').evaluate(v=>v.currentTime),147);
      await page.keyboard.press('j');await page.keyboard.press('Shift+ArrowRight');assert.equal(await page.locator('#nativePlayer').evaluate(v=>v.currentTime),167);
      await page.keyboard.press('5');assert.equal(await page.locator('#nativePlayer').evaluate(v=>v.currentTime),900);
      await page.keyboard.press(']');assert.equal(await page.locator('#nativePlayer').evaluate(v=>v.playbackRate),1.75);
      const timeline=await page.locator('#timelineBar').boundingBox();await page.mouse.move(timeline.x+timeline.width*.25,timeline.y+2);await page.mouse.down();await page.mouse.move(timeline.x+timeline.width*.75,timeline.y+2);await page.mouse.up();
      assert(Math.abs(await page.locator('#nativePlayer').evaluate(v=>v.currentTime)-1350)<15,'Rust pointer timeline drag seeks');
      const rewind = await page.locator('#centerRewind').boundingBox();
      const forward = await page.locator('#centerForward').boundingBox();
      assert(rewind.x < forward.x, 'Rewind stays left of forward in the Arabic interface');
      const tapSide = async (side, count = 2) => page.locator('#gesturesLayer').evaluate((layer, {side, count}) => {
        const rect = layer.getBoundingClientRect();
        for (let tap = 0; tap < count; tap++) {
          for (const type of ['pointerdown', 'pointerup']) {
            layer.dispatchEvent(new PointerEvent(type, {
              pointerType: 'touch', pointerId: 1,
              clientX: rect.left + rect.width * side, clientY: rect.top + rect.height / 2,
            }));
          }
        }
      }, {side, count});
      await tapSide(.5, 1);
      const beforeTaps = await page.locator('#nativePlayer').evaluate(v => v.currentTime);
      await tapSide(.75);
      assert.equal(await page.locator('#nativePlayer').evaluate(v => v.currentTime), beforeTaps + 10);
      await tapSide(.25);
      assert.equal(await page.locator('#nativePlayer').evaluate(v => v.currentTime), beforeTaps);
      assert.equal(await page.locator('.seek-ripple').count(), 0);
      await page.mouse.move(timeline.x + timeline.width * .5, timeline.y);
      await page.locator('#playerMoreToolsBtn').click();
      assert.equal(await page.locator('[data-player-action="boost"]').count(), 0);
      await page.keyboard.press('Escape');
      await page.locator('#playerCanvas').focus();await page.keyboard.press('Shift+/');assert(await page.locator('.controls-guide').isVisible());await page.keyboard.press('Escape');
      await page.locator('#dismissPlayerBtn').click();await context.close();
      console.log('PASS Rust source switching: staged lookup, settings preservation, failed metadata, failed media rollback, cancellation, shortcuts and pointer seeking');
    }
    {
      const{context,page}=await open('krmzi');
      await page.locator('#seriesGrid [data-url="'+seriesUrl+'"]').click();await page.locator('.sheet-episode-btn').first().click();
      await page.waitForFunction(()=>document.getElementById('nativePlayer').src.endsWith('/hd.mp4'));await page.evaluate(()=>{window.__ready();window.__media.time=123;});
      const before=requests.length;await page.locator('#openServersSheetBtn').click();await page.locator('[data-player-action="embed"][data-index="0"]').click();
      await page.waitForFunction(()=>document.getElementById('nativePlayer').src.endsWith('/hd-mirror.mp4'));
      assert(requests.slice(before).some(request=>request.includes('server=mirror-1')),'Selected OK mirror is resolved explicitly');
      await page.evaluate(()=>window.__ready());assert.equal(await page.locator('#nativePlayer').evaluate(v=>v.currentTime),123);
      await page.locator('#openServersSheetBtn').click();await page.locator('[data-player-action="discover"]').click();
      await page.waitForFunction(()=>document.getElementById('nativePlayer').src.includes('test-720.mp4'));
      await page.evaluate(()=>window.__ready());assert.equal(await page.locator('#nativePlayer').evaluate(v=>v.currentTime),123,'Source discovery works even outside the initial merged catalogue');
      await context.close();console.log('PASS Rust source discovery and actual selected OK mirror switching');
    }
    {
      const{context,page}=await open('krmzi');
      await page.locator('#seriesGrid .series-tile').first().click();await page.locator('.sheet-episode-btn').first().click();await page.waitForFunction(()=>document.getElementById('nativePlayer').src.endsWith('/hd.mp4'));
      await page.evaluate(()=>{window.__ready();window.__media.time=321;});await page.locator('#dismissPlayerBtn').click();
      await page.locator('#seriesGrid .series-tile').first().click();await page.locator('.sheet-episode-btn').first().click();await page.waitForFunction(()=>document.getElementById('nativePlayer').src.endsWith('/hd.mp4'));await page.evaluate(()=>window.__ready());
      assert.equal(await page.locator('#nativePlayer').evaluate(v=>v.currentTime),321,'Reopening an episode from its original series context keeps the newest history position');
      await page.locator('#dismissPlayerBtn').click();await page.locator('[data-source="cinemana"]').click();await page.locator('#seriesGrid [data-url="cinemana:700"]').click();await page.locator('.sheet-episode-btn').first().click();await page.waitForFunction(()=>document.getElementById('nativePlayer').src.includes('test-720.mp4'));
      await page.evaluate(()=>{window.__ready();window.__media.time=456;});await page.locator('#playerFavBtn').click();await page.locator('#dismissPlayerBtn').click();await page.locator('#openLibraryBtn').click();
      const saved=page.locator('#libraryGrid [data-url="cinemana:700"]');assert.equal(await saved.getAttribute('data-action'),'episode','Cinemana history opens the episode, not the whole series');await saved.click();await page.waitForFunction(()=>document.getElementById('nativePlayer').src.includes('test-720.mp4'));await page.evaluate(()=>window.__ready());assert.equal(await page.locator('#nativePlayer').evaluate(v=>v.currentTime),456);
      await page.locator('#dismissPlayerBtn').click();await page.locator('[data-library="favs"]').click();assert.equal(await page.locator('#libraryGrid [data-url="cinemana:700"]').getAttribute('data-action'),'episode','Saved Cinemana favorite remains playable');
      await context.close();console.log('PASS Rust library: contextual history resume, Cinemana episode identity, favorites and resume');
    }
    {
      const{context,page}=await open('all');delayCinemana=450;await page.locator('#openSearchBtn').click();await page.locator('#overlaySearchInput').fill('التجربة');await page.waitForTimeout(300);await page.keyboard.press('Escape');await page.waitForTimeout(500);
      const before=requests.length;await page.locator('#openSearchBtn').click();await page.waitForFunction(()=>document.querySelectorAll('#searchGrid .series-tile').length===12);assert(requests.slice(before).some(request=>request.startsWith('/api/cinemana?')),'Reopening an aborted query retries its provider request');
      await page.keyboard.press('Escape');delayCinemana=0;await page.locator('[data-source="krmzi"]').click();await page.locator('#openSearchBtn').click();await page.waitForFunction(()=>{const g=document.getElementById('searchGrid');return g&&g.querySelectorAll('.series-tile').length>=1&&g.textContent.includes('التجربة')&&!g.textContent.includes('حكاية سينمانا');});assert(!(await page.locator('#searchGrid').textContent()).includes('حكاية سينمانا'),'Changing source invalidates previous search results');
      // Search Overhaul: ranking, Arabic->English fallback, kind filters, clear/recent, recovery card
      await page.locator('#overlaySearchInput').fill('لايوجد_اطلاقا');
      await page.locator('#overlaySearchInput').press('Enter');
      await page.locator('#searchRecoveryCard [data-action="search-all-sources"]').waitFor();
      await page.locator('#searchRecoveryCard [data-action="search-all-sources"]').click();
      await page.waitForFunction(()=>document.querySelector('[data-source="all"].active'));
      await page.locator('#overlaySearchInput').fill('The Batman');
      await page.locator('#overlaySearchInput').press('Enter');
      await page.waitForFunction(()=>document.querySelector('#searchGrid .series-tile[data-url="cinemana:880"]'));
      assert.equal(await page.locator('#searchGrid .series-tile').first().getAttribute('data-url'),'cinemana:880','Exact movie title ranks first even above Krmzi series and older titles');
      await page.locator('#overlaySearchInput').fill('اوبنهايمر');
      await page.locator('#overlaySearchInput').press('Enter');
      await page.waitForFunction(()=>document.querySelector('#searchGrid .series-tile[data-url="cinemana:882"]'));
      assert.equal(await page.locator('#searchGrid .series-tile').first().getAttribute('data-url'),'cinemana:882','Arabic transliteration falls back transparently to English title on Cinemana');
      await page.locator('[data-search-kind="movie"]').click();
      await page.waitForFunction(()=>Array.from(document.querySelectorAll('#searchGrid .series-tile')).every(el=>el.getAttribute('data-action')==='movie'||el.getAttribute('data-action')==='episode'));
      assert(!(await page.locator('#searchGrid').textContent()).includes('مسلسل التجربة'),'Movies filter removes Krmzi series noise');
      await page.locator('[data-search-kind="all"]').click();
      await page.locator('#clearSearchBtn').click();
      await page.waitForFunction(()=>!document.getElementById('searchRecentWrap').classList.contains('hidden'));
      assert((await page.locator('#searchRecentList').textContent()).includes('اوبنهايمر'),'Submitted search saves into recent searches');
      await page.locator('#clearRecentSearchesBtn').click();
      await page.waitForFunction(()=>document.getElementById('searchRecentWrap').classList.contains('hidden'));
      await context.close();console.log('PASS Rust search: ranking, Arabic fallback, type filters, recent history, recovery, reopen & source invalidation');
    }
    {
      const{context,page}=await open('krmzi',{width:390,height:844},true);
      await page.locator('#seriesGrid .series-tile').first().click();await page.locator('.sheet-episode-btn').first().click();
      await page.waitForFunction(()=>document.getElementById('nativePlayer').src.endsWith('/hd.mp4'));
      await page.evaluate(()=>{window.__media.bufferedEnd=1;document.getElementById('nativePlayer').dispatchEvent(new Event('progress'));});
      await page.clock.runFor(8500);assert((await page.locator('#nativePlayer').evaluate(v=>v.src)).endsWith('/sd.mp4'),'A metadata/progress event does not disable bounded startup fallback');
      await page.evaluate(()=>{window.__ready();window.__media.paused=true;});const loads=await page.evaluate(()=>window.__media.loads.length);
      await page.clock.runFor(20000);assert.equal(await page.evaluate(()=>window.__media.loads.length),loads,'Intentional pause does not change server');
      await page.locator('#dismissPlayerBtn').click();await context.close();console.log('PASS Rust watchdog: bounded stalled startup and intentional pause');
    }
    {
      const context=await browser.newContext({viewport:{width:390,height:844},serviceWorkers:'block'});await context.addInitScript(mediaMock,'krmzi');
      await context.addInitScript(()=>{
        window.Hls=class{
          static isSupported(){return true;}
          static Events={MANIFEST_PARSED:'manifest',LEVEL_SWITCHED:'level',ERROR:'error'};
          constructor(){window.__engine=this;this.handlers={};this.levels=[{height:480},{height:720},{height:1080}];}
          on(name,fn){this.handlers[name]=fn;}
          loadSource(url){this.source=url;}
          attachMedia(video){video.src='blob:fake-media';this.handlers.manifest('manifest',{});}
          startLoad(position){this.position=position;}
          destroy(){this.destroyed=true;this.handlers={};}
        };
      });
      await context.route('**/vendor/hls/hls.min.js',route=>route.fulfill({contentType:'application/javascript',body:''}));
      const page=await context.newPage();page.on('pageerror',error=>failures.push(error.message));
      await page.route('**/api/episode/detail?**',async route=>{const response=await route.fetch();const data=await response.json();data.directStreams.unshift({quality:'تلقائي',type:'hls',url:'https://vd196.okcdn.ru/master.m3u8'});await route.fulfill({response,json:data});});
      await page.goto(origin);await page.locator('#seriesGrid .series-tile').first().click();await page.locator('.sheet-episode-btn').first().click();
      await page.waitForFunction(()=>!!window.__engine?.source);assert((await page.evaluate(()=>window.__engine.source)).startsWith('/api/media?url='));assert.equal(await page.evaluate(()=>window.__engine.startLevel),1,'Adaptive starts at720p');
      await page.evaluate(()=>{window.__ready();window.__media.time=137;});await page.locator('#openQualitySheetBtn').click();await page.locator('[data-player-action="quality-hls"][data-index="2"]').click();
      assert.equal(await page.evaluate(()=>window.__engine.nextLevel),2);assert.equal(await page.locator('#nativePlayer').evaluate(v=>v.currentTime),137,'HLS level switch preserves buffered timeline');
      await page.locator('#openQualitySheetBtn').click();await page.locator('[data-player-action="quality-auto"]').click();assert.equal(await page.evaluate(()=>window.__engine.nextLevel),-1);
      await page.locator('#dismissPlayerBtn').click();assert.equal(await page.evaluate(()=>window.__engine.destroyed),true,'Closing destroys HLS downloads');
      await page.evaluate(()=>document.getElementById('nativePlayer').dispatchEvent(new Event('error')));assert(await page.locator('#mobilePlayer').evaluate(el=>el.classList.contains('hidden')),'Late media error cannot reopen a closed HLS player');
      cinemanaHls=true;await page.locator('[data-source="cinemana"]').click();await page.locator('#seriesGrid [data-url="cinemana:700"]').click();await page.locator('.sheet-episode-btn').first().click();await page.waitForFunction(()=>window.__engine?.source==='https://cdn.shabakaty.com/master.m3u8');assert.equal(await page.evaluate(()=>window.__engine.source),'https://cdn.shabakaty.com/master.m3u8','Cinemana HLS bypasses the OK-only media proxy');await page.locator('#dismissPlayerBtn').click();cinemanaHls=false;
      await context.close();console.log('PASS Rust adaptive player: scoped HLS session, same-origin OK manifest, level switching, Auto and cleanup');
    }
    for(const viewport of [{width:1280,height:720},{width:1920,height:1080}]){
      const context=await browser.newContext({viewport,serviceWorkers:'block'});await context.addInitScript(mediaMock,'krmzi');await context.addInitScript(()=>localStorage.setItem('otium_display','tv'));await context.route('https://**/*',route=>route.abort());
      const page=await context.newPage();page.on('pageerror',error=>failures.push(error.message));await page.goto(origin);await page.locator('#seriesGrid .series-tile').first().waitFor();
      assert.equal(await page.locator('html').getAttribute('data-display'),'tv');await page.locator('#seriesGrid .series-tile').first().focus();await page.keyboard.press('ArrowDown');
      const focused=await page.evaluate(()=>document.activeElement?.tagName);assert.equal(focused,'BUTTON');
      await page.locator('#seriesGrid .series-tile').first().click();await page.locator('.sheet-episode-btn').first().click();await page.waitForFunction(()=>document.getElementById('nativePlayer').src.endsWith('/hd.mp4'));await page.evaluate(()=>window.__ready());
      await page.locator('#playerCanvas').focus();await page.keyboard.press('Enter');assert.equal(await page.locator('#nativePlayer').evaluate(v=>v.paused),true);await page.keyboard.press('Enter');
      await page.locator('#centerPlay').focus();await page.keyboard.press('ArrowRight');assert.notEqual(await page.evaluate(()=>document.activeElement.id),'centerPlay','TV arrows move focus between playback controls');
      await page.evaluate(()=>{window.__media.time=120;for(const id of ['playerHudTop','playerHudBottom','hudCenter'])document.getElementById(id).classList.add('hud-hidden');});
      await page.locator('#playerCanvas').focus();await page.keyboard.press('ArrowLeft');assert.equal(await page.locator('#nativePlayer').evaluate(v=>v.currentTime),110,'Hidden HUD remote seek uses ten seconds');
      await page.evaluate(()=>{for(const id of ['playerHudTop','playerHudBottom','hudCenter'])document.getElementById(id).classList.add('hud-hidden');});await page.keyboard.press('ArrowUp');assert.equal(await page.evaluate(()=>document.activeElement.id),'centerPlay');
      await page.keyboard.press('Escape');assert(await page.locator('#mobilePlayer').evaluate(el=>el.classList.contains('hidden')));
      await context.close();console.log(`PASS Rust television ${viewport.width}x${viewport.height}: layout, remote focus, player D-pad, seeking and Back`);
    }
    assert.deepEqual(failures,[],'No browser runtime errors');
  }finally{await browser.close();await new Promise(resolve=>server.close(resolve));}
}

if(require.main===module)run().catch(error=>{console.error(error);server.close();process.exitCode=1;});
module.exports={run,mediaMock,server,requests,failures,episodes,series,episodeUrl};
