layout (std140) uniform ubo_camera {
    mat4 view;
    mat4 projection;
    vec4 position; // needs to be vec4 for layout
} camera;

struct Camera 
{
    mat4 view;
    mat4 projection;
    vec3 position;
};

Camera getCamera() {
    return Camera(
        camera.view,
        camera.projection,
        camera.position.xyz
    );
}