#version 300 es
precision mediump float;

%% INCLUDE_COMMON_MATH %%
%% INCLUDE_COMMON_CAMERA %%

out vec4 color; 
flat in vec3 v_color; 
in vec3 v_normal; 
in vec3 v_vertex; 

void main() {
    // todo - fix lighting
    vec3 normal = normalize(v_normal);
    vec3 light_color = vec3(1.0, 1.0, 1.0);
    vec3 light_result = vec3(0.1, 0.1, 0.1);
    vec3 light_pos = vec3(100.0, 100.0, -100.0);
    vec3 light_dir = normalize(light_pos - v_vertex);
    float diffuse = max(0.0, dot(light_dir, normal));
    light_result += diffuse * light_color;
    color = vec4(v_color * light_result, 1.0);
}