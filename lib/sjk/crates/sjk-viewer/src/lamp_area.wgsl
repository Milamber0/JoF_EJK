// Float reconstruction and interpolated shading normals must not let a source
// illuminate its own geometric plane. This tolerance is 1/20 of a world unit.
fn lamp_front_side(world:vec3<f32>,center:vec3<f32>,emitter_normal:vec3<f32>)->bool {
    return dot(world-center,emitter_normal)>0.05;
}
// A clipped convex rectangle has at most one entry and one exit at the horizon.
// Accumulate its four edges directly instead of indexing a variable-size polygon
// inside the per-lamp loop, avoiding dynamically indexed temporary storage.
struct LampAreaIntegral { value:f32, entering:vec3<f32>, leaving:vec3<f32>, clipped:bool };
fn lamp_edge_integral(a:vec3<f32>,b:vec3<f32>,normal:vec3<f32>)->f32 {
    let na=a/max(length(a),1e-6); let nb=b/max(length(b),1e-6);
    let edge=cross(na,nb); let cosine=clamp(dot(na,nb),-1.0,1.0);
    let x=abs(cosine);
    // Degree-eight fit of acos(x)/sqrt(1-x*x) on [0,1], endpoint limit 1.
    // The obtuse half follows from acos(-x)=pi-acos(x). Dense GPU angle tests
    // bound the approximation independently of the area-light integration tests.
    var factor=1.5707960460+x*(-0.9999735370+x*(0.7847802513+x*(-0.6604219512+x*(0.5550385794+x*(-0.4213067398+x*(0.2502761748+x*(-0.0965941743+x*(0.0174055491))))))));
    if cosine<0.0 { factor=3.14159265359/max(length(edge),1e-6)-factor; }
    return dot(normal,edge)*factor;
}
fn lamp_clipped_edge(a:vec3<f32>,b:vec3<f32>,normal:vec3<f32>,
    integral:ptr<function,LampAreaIntegral>) {
    let ha=dot(a,normal); let hb=dot(b,normal);
    if ha>0.0 && hb>0.0 {
        (*integral).value+=lamp_edge_integral(a,b,normal);
    } else if ha>0.0 {
        let q=mix(a,b,ha/(ha-hb));
        (*integral).value+=lamp_edge_integral(a,q,normal);
        (*integral).leaving=q; (*integral).clipped=true;
    } else if hb>0.0 {
        let q=mix(a,b,ha/(ha-hb));
        (*integral).value+=lamp_edge_integral(q,b,normal);
        (*integral).entering=q; (*integral).clipped=true;
    }
}
// Diffuse spherical polygon form factor with exact receiver-horizon clipping.
fn lamp_area_irradiance(world:vec3<f32>,normal:vec3<f32>,center:vec3<f32>,
    u:vec3<f32>,v:vec3<f32>)->f32 {
    let a=center-u-v-world; let b=center+u-v-world;
    let c=center+u+v-world; let d=center-u+v-world;
    var integral=LampAreaIntegral(0.0,vec3(0.0),vec3(0.0),false);
    lamp_clipped_edge(a,b,normal,&integral);
    lamp_clipped_edge(b,c,normal,&integral);
    lamp_clipped_edge(c,d,normal,&integral);
    lamp_clipped_edge(d,a,normal,&integral);
    if integral.clipped {
        integral.value+=lamp_edge_integral(integral.leaving,integral.entering,normal);
    }
    return min(abs(integral.value)*0.5,3.14159265);
}

// At small source angles, the differential-area limit avoids four spherical-edge
// integrals. This depends only on source/receiver geometry, never camera distance.
// The smooth overlap keeps lighting continuous as a receiver moves past the limit.
fn lamp_area_form(world:vec3<f32>,normal:vec3<f32>,center:vec3<f32>,
    emitter_normal:vec3<f32>,u:vec3<f32>,v:vec3<f32>,inverse_area:f32)->f32 {
    let to=center-world;
    let d2=dot(to,to);
    let extent2=dot(u,u)+dot(v,v);
    let far=smoothstep(64.0*extent2,144.0*extent2,d2);
    let l=to*inverseSqrt(max(d2,1e-6));
    var emission_cosine=max(dot(-l,emitter_normal),0.0);
    // Translucent fixed panels emit from both sides, retaining their finite area.
    if dot(emitter_normal,emitter_normal)<0.5 {
        emission_cosine=abs(dot(l,normalize(cross(u,v))));
    }
    let point=emission_cosine*max(dot(l,normal),0.0)/max(d2,1e-6);
    if far>=1.0 { return point; }
    return mix(lamp_area_irradiance(world,normal,center,u,v)*inverse_area,point,far);
}
