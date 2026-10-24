struct Vertex {
 @builtin(position) position: vec4<f32>,
 @location(0) uv: vec2<f32>,
 @location(1) color: vec4<f32>,
 @location(2) dark: vec4<f32>,
 @location(3) pma: f32,
};
@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var atlas_sampler: sampler;
@vertex fn vs(@location(0) position:vec2<f32>,@location(1) uv:vec2<f32>,@location(2) color:vec4<f32>,@location(3) dark:vec4<f32>,@location(4) pma:f32)->Vertex {
 var out:Vertex;out.position=vec4(position,0.,1.);out.uv=uv;out.color=color;out.dark=dark;out.pma=pma;return out;
}
@fragment fn fs(input:Vertex)->@location(0) vec4<f32> {
 let tex=textureSample(atlas,atlas_sampler,input.uv);
 let rgb=((tex.a-1.)*input.dark.a+1.-tex.rgb)*input.dark.rgb+tex.rgb*input.color.rgb;
 let alpha=tex.a*input.color.a;
 return vec4(rgb*select(alpha,1.,input.pma>0.5),alpha);
}
