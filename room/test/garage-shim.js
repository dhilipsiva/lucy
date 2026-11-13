// API orchestration fixture only (roadmap/N8i.md): it cannot establish headset compatibility,
// haptics, hit-test, plane detection or comfort. A fake navigator.xr for the garage: two
// controllers with gamepads and haptic actuators, immersive-vr and immersive-ar, hit-test
// results on the floor, and one detected wall 1.5 m ahead.
window.__xrRequests=[];window.__xrTrace=[];window.__pulses=[];window.__clearColors=[];window.__xrFrame=null;
const identity=(x=0,y=0,z=0)=>new Float32Array([1,0,0,0,0,1,0,0,0,0,1,0,x,y,z,1]);
const projection=new Float32Array([1,0,0,0,0,1,0,0,0,0,-1.003,-1,0,0,-0.1,0]);
// the wall's plane space: plane +y turned to face the viewer, standing at z=-1.5
const wallPose=new Float32Array([1,0,0,0, 0,0,1,0, 0,-1,0,0, 0,1,-1.5,1]);
window.__hitAt=[0,0,-0.5];
WebGL2RenderingContext.prototype.makeXRCompatible=function(){window.__xrCompatible=true;return Promise.resolve();};
for(const name of ['clear','viewport']){const original=WebGL2RenderingContext.prototype[name];WebGL2RenderingContext.prototype[name]=function(...args){if(window.__xrFrame)window.__xrFrame.calls.push({name,args});return original.apply(this,args);};}
{const original=WebGL2RenderingContext.prototype.clearColor;WebGL2RenderingContext.prototype.clearColor=function(...args){if(window.__xrFrame)window.__clearColors.push(args);return original.apply(this,args);};}
window.XRWebGLLayer=class {constructor(session,gl){this.framebuffer=null;this.framebufferWidth=1280;this.framebufferHeight=800;}getViewport(view){return {x:view.eye==='left'?0:640,y:0,width:640,height:800};}};
const pad=(hand)=>({axes:[0,0,0,0],buttons:Array.from({length:7},()=>({pressed:false,touched:false,value:0})),hapticActuators:[{pulse(intensity,ms){window.__pulses.push({hand,intensity,ms});return Promise.resolve(true);}}]});
window.__pads={left:pad('left'),right:pad('right')};
class Session {
 constructor(mode,options){this.mode=mode;this.enabledFeatures=(options?.optionalFeatures||[]).filter(f=>f!=='camera-access');this.ended=false;
  this.inputSources=[{handedness:'right',targetRaySpace:{hand:'right'},gamepad:window.__pads.right},{handedness:'left',targetRaySpace:{hand:'left'},gamepad:window.__pads.left}];}
 updateRenderState(state){this.renderState=state;}
 requestReferenceSpace(type){return Promise.resolve({type});}
 requestHitTestSource(options){return Promise.resolve({hand:options.space.hand});}
 requestAnimationFrame(cb){if(this.ended)return 0;return setTimeout(()=>{if(this.ended)return;
  const frame={
   getViewerPose:()=>({transform:{matrix:identity(0,1.6,0)},views:[{eye:'left',transform:{matrix:identity(-0.032,1.6,0)},projectionMatrix:projection},{eye:'right',transform:{matrix:identity(0.032,1.6,0)},projectionMatrix:projection}]}),
   getPose:(space)=>({transform:{matrix:space&&space.id==='wall'?wallPose:identity(0,1.6,0)}}),
   getHitTestResults:(src)=>[{getPose:()=>({transform:{matrix:identity(...window.__hitAt)}})}],
   detectedPlanes:this.mode==='immersive-ar'?new Set([{orientation:'vertical',planeSpace:{id:'wall'},polygon:[{x:-1,y:0,z:-1},{x:1,y:0,z:-1},{x:1,y:0,z:1},{x:-1,y:0,z:1}]}]):undefined,
  };
  window.__xrCurrent=frame;window.__xrFrame={calls:[]};cb(performance.now(),frame);window.__xrTrace.push(window.__xrFrame);window.__xrFrame=null;},30);}
 end(){this.ended=true;return new Promise(resolve=>setTimeout(()=>{this.onend?.(new Event('end'));resolve();},0));}
}
Object.defineProperty(navigator,'xr',{configurable:true,value:{isSessionSupported:()=>Promise.resolve(true),requestSession(mode,options){window.__xrRequests.push({mode,options,activation:navigator.userActivation.isActive});window.__xrSession=new Session(mode,options);return Promise.resolve(window.__xrSession);}}});
window.__select=(hand='right')=>{const s=window.__xrSession;s.onselectstart({frame:window.__xrCurrent,inputSource:s.inputSources.find(i=>i.handedness===hand)});};
