// API orchestration fixture only: this cannot establish headset compatibility or comfort.
window.__xrTrace=[];window.__xrRequests=[];window.__xrFrame=null;window.__xrSelectCount=0;
const identity=(x=0,y=0,z=0)=>new Float32Array([1,0,0,0,0,1,0,0,0,0,1,0,x,y,z,1]);
window.__xrRay=identity(0,1.6,0);
const projection=new Float32Array([1,0,0,0,0,1,0,0,0,0,-1.003,-1,0,0,-0.1,0]);
const makeCompatible=WebGL2RenderingContext.prototype.makeXRCompatible;
WebGL2RenderingContext.prototype.makeXRCompatible=function(){window.__xrCompatible=true;return Promise.resolve();};
for(const name of ['clear','viewport']){const original=WebGL2RenderingContext.prototype[name];WebGL2RenderingContext.prototype[name]=function(...args){if(window.__xrFrame)window.__xrFrame.calls.push({name,args});return original.apply(this,args);};}
window.XRWebGLLayer=class {constructor(session,gl){this.framebuffer=null;this.framebufferWidth=1280;this.framebufferHeight=800;this.gl=gl;}getViewport(view){return {x:view.eye==='left'?0:640,y:0,width:640,height:800};}};
class Session {
 constructor(){this.inputSources=[{targetRaySpace:{},handedness:'right',gamepad:null}];this.ended=false;}
 updateRenderState(state){this.renderState=state;}
 requestReferenceSpace(type){return Promise.resolve({type});}
 requestAnimationFrame(cb){if(this.ended)return 0;return setTimeout(()=>{if(this.ended)return;const frame={getViewerPose:()=>({transform:{matrix:identity(0,1.6,0)},views:[{eye:'left',transform:{matrix:identity(-0.032,1.6,0)},projectionMatrix:projection},{eye:'right',transform:{matrix:identity(0.032,1.6,0)},projectionMatrix:projection}]}),getPose:()=>({transform:{matrix:window.__xrRay}})};window.__xrCurrent=frame;window.__xrFrame={calls:[]};cb(performance.now(),frame);window.__xrTrace.push(window.__xrFrame);window.__xrFrame=null;},30);}
 end(){this.ended=true;return new Promise(resolve=>setTimeout(()=>{this.onend?.(new Event('end'));resolve();},0));}
}
Object.defineProperty(navigator,'xr',{configurable:true,value:{isSessionSupported:()=>Promise.resolve(true),requestSession(mode,options){window.__xrRequests.push({mode,options,activation:navigator.userActivation.isActive});window.__xrSession=new Session();return Promise.resolve(window.__xrSession);}}});
window.__selectRay=(matrix)=>{window.__xrRay=new Float32Array(matrix);window.__xrSelectCount++;window.__xrSession.onselectstart({frame:window.__xrCurrent,inputSource:window.__xrSession.inputSources[0]});};
