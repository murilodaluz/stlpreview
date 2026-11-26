import math
import shutil
import struct
import zipfile
import zlib
from pathlib import Path

Vec = tuple[float, float, float]
Triangle = tuple[Vec, Vec, Vec]
Mesh = list[Triangle]
Polygon = list[Vec]
IndexedTriangle = tuple[int, int, int]
Transform = tuple[float, float, float, float, float, float, float, float, float, float, float, float]

ROOT: Path = Path(__file__).resolve().parent.parent / "fixtures" / "library"
MODEL_NAMESPACE: str = "http://schemas.microsoft.com/3dmanufacturing/core/2015/02"
CONTENT_TYPES_XML: str = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
    '<Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>'
    "</Types>\n"
)
RELS_XML: str = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
    '<Relationship Target="/3D/3dmodel.model" Id="rel0" '
    'Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>'
    "</Relationships>\n"
)
IDENTITY_TRANSFORM: Transform = (1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0)
ORIGIN: Vec = (0.0, 0.0, 0.0)
BINARY_TRIANGLE: struct.Struct = struct.Struct("<12fH")
NOTES_TEXT: str = "Sample library notes. This file is not a model and must be ignored by the scanner.\n"
HELMET_README_TEXT: str = "Knight helmet project parts. This file is not a model and must be ignored by the scanner.\n"


def vec_add(a: Vec, b: Vec) -> Vec:
    return (a[0] + b[0], a[1] + b[1], a[2] + b[2])


def vec_sub(a: Vec, b: Vec) -> Vec:
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def vec_scale(a: Vec, factor: float) -> Vec:
    return (a[0] * factor, a[1] * factor, a[2] * factor)


def vec_dot(a: Vec, b: Vec) -> float:
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def vec_cross(a: Vec, b: Vec) -> Vec:
    return (
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    )


def vec_normalize(a: Vec) -> Vec:
    length: float = math.sqrt(vec_dot(a, a))
    if length == 0.0:
        return ORIGIN
    return vec_scale(a, 1.0 / length)


def triangle_centroid(tri: Triangle) -> Vec:
    return vec_scale(vec_add(vec_add(tri[0], tri[1]), tri[2]), 1.0 / 3.0)


def triangle_normal(tri: Triangle) -> Vec:
    return vec_normalize(vec_cross(vec_sub(tri[1], tri[0]), vec_sub(tri[2], tri[0])))


def orient_outward(tri: Triangle, inside: Vec) -> Triangle:
    outward_offset: Vec = vec_sub(triangle_centroid(tri), inside)
    if vec_dot(triangle_normal(tri), outward_offset) < 0.0:
        return (tri[0], tri[2], tri[1])
    return tri


def fan_triangles(polygon: Polygon) -> Mesh:
    first: Vec = polygon[0]
    return [(first, polygon[index], polygon[index + 1]) for index in range(1, len(polygon) - 1)]


def convex_mesh(faces: list[Polygon], inside: Vec) -> Mesh:
    mesh: Mesh = []
    for face in faces:
        for tri in fan_triangles(face):
            mesh.append(orient_outward(tri, inside))
    return mesh


def box_mesh(size_x: float, size_y: float, size_z: float) -> Mesh:
    c000: Vec = (0.0, 0.0, 0.0)
    c100: Vec = (size_x, 0.0, 0.0)
    c110: Vec = (size_x, size_y, 0.0)
    c010: Vec = (0.0, size_y, 0.0)
    c001: Vec = (0.0, 0.0, size_z)
    c101: Vec = (size_x, 0.0, size_z)
    c111: Vec = (size_x, size_y, size_z)
    c011: Vec = (0.0, size_y, size_z)
    faces: list[Polygon] = [
        [c000, c100, c110, c010],
        [c001, c101, c111, c011],
        [c000, c010, c011, c001],
        [c100, c101, c111, c110],
        [c000, c001, c101, c100],
        [c010, c110, c111, c011],
    ]
    return convex_mesh(faces, (size_x / 2.0, size_y / 2.0, size_z / 2.0))


def pyramid_mesh(base_size: float, height: float) -> Mesh:
    half: float = base_size / 2.0
    corner_a: Vec = (-half, -half, 0.0)
    corner_b: Vec = (half, -half, 0.0)
    corner_c: Vec = (half, half, 0.0)
    corner_d: Vec = (-half, half, 0.0)
    apex: Vec = (0.0, 0.0, height)
    faces: list[Polygon] = [
        [corner_a, corner_b, corner_c, corner_d],
        [corner_a, corner_b, apex],
        [corner_b, corner_c, apex],
        [corner_c, corner_d, apex],
        [corner_d, corner_a, apex],
    ]
    return convex_mesh(faces, (0.0, 0.0, height / 4.0))


def wedge_mesh(length: float, width: float, height: float) -> Mesh:
    a: Vec = (0.0, 0.0, 0.0)
    b: Vec = (length, 0.0, 0.0)
    c: Vec = (length, width, 0.0)
    d: Vec = (0.0, width, 0.0)
    e: Vec = (0.0, 0.0, height)
    f: Vec = (0.0, width, height)
    faces: list[Polygon] = [
        [a, b, c, d],
        [a, d, f, e],
        [a, e, b],
        [d, c, f],
        [b, e, f, c],
    ]
    return convex_mesh(faces, (length / 3.0, width / 2.0, height / 3.0))


def sphere_point(radius: float, longitudes: int, latitudes: int, lat_index: int, lon_index: int) -> Vec:
    theta: float = math.pi * lat_index / latitudes
    phi: float = 2.0 * math.pi * lon_index / longitudes
    return (
        radius * math.sin(theta) * math.cos(phi),
        radius * math.sin(theta) * math.sin(phi),
        radius * math.cos(theta),
    )


def sphere_mesh(radius: float, longitudes: int, latitudes: int) -> Mesh:
    top: Vec = (0.0, 0.0, radius)
    bottom: Vec = (0.0, 0.0, -radius)
    mesh: Mesh = []
    for lon in range(longitudes):
        lon_next: int = (lon + 1) % longitudes
        first_ring: Vec = sphere_point(radius, longitudes, latitudes, 1, lon)
        first_ring_next: Vec = sphere_point(radius, longitudes, latitudes, 1, lon_next)
        last_ring: Vec = sphere_point(radius, longitudes, latitudes, latitudes - 1, lon)
        last_ring_next: Vec = sphere_point(radius, longitudes, latitudes, latitudes - 1, lon_next)
        mesh.append(orient_outward((top, first_ring, first_ring_next), ORIGIN))
        mesh.append(orient_outward((last_ring, bottom, last_ring_next), ORIGIN))
        for lat in range(1, latitudes - 1):
            a: Vec = sphere_point(radius, longitudes, latitudes, lat, lon)
            b: Vec = sphere_point(radius, longitudes, latitudes, lat + 1, lon)
            c: Vec = sphere_point(radius, longitudes, latitudes, lat + 1, lon_next)
            d: Vec = sphere_point(radius, longitudes, latitudes, lat, lon_next)
            mesh.append(orient_outward((a, b, c), ORIGIN))
            mesh.append(orient_outward((a, c, d), ORIGIN))
    return mesh


def torus_point(major: float, minor: float, u_steps: int, v_steps: int, u_index: int, v_index: int) -> Vec:
    u: float = 2.0 * math.pi * u_index / u_steps
    v: float = 2.0 * math.pi * v_index / v_steps
    ring_radius: float = major + minor * math.cos(v)
    return (ring_radius * math.cos(u), ring_radius * math.sin(u), minor * math.sin(v))


def torus_outward(tri: Triangle, major: float) -> Triangle:
    center: Vec = triangle_centroid(tri)
    angle: float = math.atan2(center[1], center[0])
    tube_center: Vec = (major * math.cos(angle), major * math.sin(angle), 0.0)
    return orient_outward(tri, tube_center)


def torus_mesh(major: float, minor: float, u_steps: int, v_steps: int) -> Mesh:
    mesh: Mesh = []
    for u_index in range(u_steps):
        u_next: int = (u_index + 1) % u_steps
        for v_index in range(v_steps):
            v_next: int = (v_index + 1) % v_steps
            a: Vec = torus_point(major, minor, u_steps, v_steps, u_index, v_index)
            b: Vec = torus_point(major, minor, u_steps, v_steps, u_next, v_index)
            c: Vec = torus_point(major, minor, u_steps, v_steps, u_next, v_next)
            d: Vec = torus_point(major, minor, u_steps, v_steps, u_index, v_next)
            mesh.append(torus_outward((a, b, c), major))
            mesh.append(torus_outward((a, c, d), major))
    return mesh


def polar_point(radius: float, theta: float, phi: float) -> Vec:
    return (
        radius * math.sin(theta) * math.cos(phi),
        radius * math.sin(theta) * math.sin(phi),
        radius * math.cos(theta),
    )


def spherical_rings(radius: float, longitudes: int, thetas: list[float]) -> list[list[Vec]]:
    return [
        [polar_point(radius, theta, 2.0 * math.pi * lon / longitudes) for lon in range(longitudes)]
        for theta in thetas
    ]


def orient_along(tri: Triangle, outward: Vec) -> Triangle:
    if vec_dot(triangle_normal(tri), outward) < 0.0:
        return (tri[0], tri[2], tri[1])
    return tri


def outward_triangles(candidates: Mesh, inside: Vec) -> Mesh:
    return [orient_outward(tri, inside) for tri in candidates if triangle_normal(tri) != ORIGIN]


def revolution_mesh(rings: list[list[Vec]]) -> Mesh:
    mesh: Mesh = []
    for upper, lower in zip(rings, rings[1:]):
        longitudes: int = len(upper)
        for lon in range(longitudes):
            lon_next: int = (lon + 1) % longitudes
            candidates: Mesh = [
                (upper[lon], lower[lon], lower[lon_next]),
                (upper[lon], lower[lon_next], upper[lon_next]),
            ]
            mesh.extend(outward_triangles(candidates, ORIGIN))
    return mesh


def disc_mesh(ring: list[Vec], center: Vec, inside: Vec) -> Mesh:
    candidates: Mesh = [(center, ring[index], ring[(index + 1) % len(ring)]) for index in range(len(ring))]
    return outward_triangles(candidates, inside)


def spherical_band_mesh(radius: float, longitudes: int, z_low: float, z_high: float, bands: int) -> Mesh:
    theta_high: float = math.acos(z_high / radius)
    theta_low: float = math.acos(z_low / radius)
    thetas: list[float] = [theta_high + (theta_low - theta_high) * index / bands for index in range(bands + 1)]
    rings: list[list[Vec]] = spherical_rings(radius, longitudes, thetas)
    inside: Vec = (0.0, 0.0, (z_low + z_high) / 2.0)
    return (
        revolution_mesh(rings)
        + disc_mesh(rings[0], (0.0, 0.0, z_high), inside)
        + disc_mesh(rings[-1], (0.0, 0.0, z_low), inside)
    )


def arc_point(radius: float, angle: float, z: float) -> Vec:
    return (radius * math.cos(angle), radius * math.sin(angle), z)


def quad_mesh(a: Vec, b: Vec, c: Vec, d: Vec, outward: Vec) -> Mesh:
    return [orient_along(tri, outward) for tri in ((a, b, c), (a, c, d))]


def arc_shell_mesh(
    outer_radius: float,
    wall: float,
    height: float,
    start_degrees: float,
    sweep_degrees: float,
    segments: int,
) -> Mesh:
    inner_radius: float = outer_radius - wall
    start: float = math.radians(start_degrees)
    end: float = math.radians(start_degrees + sweep_degrees)
    angles: list[float] = [start + (end - start) * index / segments for index in range(segments + 1)]
    mesh: Mesh = []
    for first, second in zip(angles, angles[1:]):
        middle: float = (first + second) / 2.0
        radial: Vec = (math.cos(middle), math.sin(middle), 0.0)
        mesh.extend(quad_mesh(
            arc_point(outer_radius, first, 0.0),
            arc_point(outer_radius, second, 0.0),
            arc_point(outer_radius, second, height),
            arc_point(outer_radius, first, height),
            radial,
        ))
        mesh.extend(quad_mesh(
            arc_point(inner_radius, first, 0.0),
            arc_point(inner_radius, second, 0.0),
            arc_point(inner_radius, second, height),
            arc_point(inner_radius, first, height),
            vec_scale(radial, -1.0),
        ))
        mesh.extend(quad_mesh(
            arc_point(outer_radius, first, height),
            arc_point(outer_radius, second, height),
            arc_point(inner_radius, second, height),
            arc_point(inner_radius, first, height),
            (0.0, 0.0, 1.0),
        ))
        mesh.extend(quad_mesh(
            arc_point(outer_radius, first, 0.0),
            arc_point(outer_radius, second, 0.0),
            arc_point(inner_radius, second, 0.0),
            arc_point(inner_radius, first, 0.0),
            (0.0, 0.0, -1.0),
        ))
    start_outward: Vec = (math.sin(start), -math.cos(start), 0.0)
    end_outward: Vec = (-math.sin(end), math.cos(end), 0.0)
    mesh.extend(quad_mesh(
        arc_point(outer_radius, start, 0.0),
        arc_point(inner_radius, start, 0.0),
        arc_point(inner_radius, start, height),
        arc_point(outer_radius, start, height),
        start_outward,
    ))
    mesh.extend(quad_mesh(
        arc_point(outer_radius, end, 0.0),
        arc_point(inner_radius, end, 0.0),
        arc_point(inner_radius, end, height),
        arc_point(outer_radius, end, height),
        end_outward,
    ))
    return mesh


def mirror_x(vertex: Vec) -> Vec:
    return (-vertex[0], vertex[1], vertex[2])


def mirror_x_mesh(mesh: Mesh) -> Mesh:
    return [(mirror_x(tri[0]), mirror_x(tri[2]), mirror_x(tri[1])) for tri in mesh]


def binary_stl_bytes(mesh: Mesh) -> bytes:
    header: bytes = b"binary STL generated by make_fixtures.py".ljust(80, b" ")
    chunks: list[bytes] = [header, struct.pack("<I", len(mesh))]
    for tri in mesh:
        normal: Vec = triangle_normal(tri)
        chunks.append(BINARY_TRIANGLE.pack(*normal, *tri[0], *tri[1], *tri[2], 0))
    return b"".join(chunks)


def format_coordinate(value: float) -> str:
    return f"{value:.6e}"


def format_vec(vector: Vec) -> str:
    return " ".join(format_coordinate(component) for component in vector)


def ascii_stl_text(name: str, mesh: Mesh) -> str:
    lines: list[str] = [f"solid {name}"]
    for tri in mesh:
        lines.append(f"  facet normal {format_vec(triangle_normal(tri))}")
        lines.append("    outer loop")
        for vertex in tri:
            lines.append(f"      vertex {format_vec(vertex)}")
        lines.append("    endloop")
        lines.append("  endfacet")
    lines.append(f"endsolid {name}")
    return "\n".join(lines) + "\n"


def indexed_mesh(mesh: Mesh) -> tuple[list[Vec], list[IndexedTriangle]]:
    lookup: dict[Vec, int] = {}
    vertices: list[Vec] = []
    triangles: list[IndexedTriangle] = []
    for tri in mesh:
        indices: list[int] = []
        for vertex in tri:
            if vertex not in lookup:
                lookup[vertex] = len(vertices)
                vertices.append(vertex)
            indices.append(lookup[vertex])
        triangles.append((indices[0], indices[1], indices[2]))
    return vertices, triangles


def format_transform(transform: Transform) -> str:
    return " ".join(f"{value:.6f}" for value in transform)


def rotation_z_translation(degrees: float, translation: Vec) -> Transform:
    angle: float = math.radians(degrees)
    cos_a: float = math.cos(angle)
    sin_a: float = math.sin(angle)
    return (
        cos_a, sin_a, 0.0,
        -sin_a, cos_a, 0.0,
        0.0, 0.0, 1.0,
        translation[0], translation[1], translation[2],
    )


def mesh_object_xml(object_id: int, mesh: Mesh) -> str:
    vertices, triangles = indexed_mesh(mesh)
    vertex_xml: str = "".join(
        f'<vertex x="{x:.6f}" y="{y:.6f}" z="{z:.6f}"/>' for x, y, z in vertices
    )
    triangle_xml: str = "".join(
        f'<triangle v1="{a}" v2="{b}" v3="{c}"/>' for a, b, c in triangles
    )
    return (
        f'<object id="{object_id}" type="model">'
        f"<mesh><vertices>{vertex_xml}</vertices><triangles>{triangle_xml}</triangles></mesh>"
        "</object>"
    )


def model_document(resources_xml: str, build_xml: str) -> bytes:
    text: str = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        f'<model unit="millimeter" xml:lang="en-US" xmlns="{MODEL_NAMESPACE}">'
        f"<resources>{resources_xml}</resources>"
        f"<build>{build_xml}</build>"
        "</model>\n"
    )
    return text.encode("utf-8")


def build_item_xml(object_id: int, transform: Transform | None) -> str:
    if transform is None:
        return f'<item objectid="{object_id}"/>'
    return f'<item objectid="{object_id}" transform="{format_transform(transform)}"/>'


def component_object_xml(object_id: int, components: list[tuple[int, Transform]]) -> str:
    component_xml: str = "".join(
        f'<component objectid="{target}" transform="{format_transform(transform)}"/>'
        for target, transform in components
    )
    return f'<object id="{object_id}" type="model"><components>{component_xml}</components></object>'


def write_3mf(path: Path, model_bytes: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("[Content_Types].xml", CONTENT_TYPES_XML)
        archive.writestr("_rels/.rels", RELS_XML)
        archive.writestr("3D/3dmodel.model", model_bytes)


def png_chunk(kind: bytes, data: bytes) -> bytes:
    body: bytes = kind + data
    return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)


def tiny_png_bytes() -> bytes:
    header: bytes = struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0)
    scanline: bytes = bytes([0, 255, 0, 0])
    return (
        b"\x89PNG\r\n\x1a\n"
        + png_chunk(b"IHDR", header)
        + png_chunk(b"IDAT", zlib.compress(scanline))
        + png_chunk(b"IEND", b"")
    )


def write_bytes(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def write_text(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def build_library() -> None:
    if ROOT.exists():
        shutil.rmtree(ROOT)

    cube: Mesh = box_mesh(20.0, 20.0, 20.0)
    cube_stl: bytes = binary_stl_bytes(cube)

    write_bytes(ROOT / "cube.stl", cube_stl)
    write_text(ROOT / "toys" / "pyramid_ascii.stl", ascii_stl_text("pyramid_ascii", pyramid_mesh(20.0, 20.0)))
    write_bytes(ROOT / "toys" / "sphere.stl", binary_stl_bytes(sphere_mesh(15.0, 32, 16)))
    write_bytes(ROOT / "toys" / "vehicles" / "wedge.stl", binary_stl_bytes(wedge_mesh(40.0, 20.0, 10.0)))
    write_bytes(ROOT / "tools" / "jigs" / "torus.stl", binary_stl_bytes(torus_mesh(15.0, 5.0, 32, 16)))

    write_3mf(
        ROOT / "tools" / "jigs" / "cube.3mf",
        model_document(
            mesh_object_xml(1, cube),
            build_item_xml(1, None),
        ),
    )

    bracket_transform: Transform = rotation_z_translation(45.0, (50.0, 20.0, 5.0))
    write_3mf(
        ROOT / "tools" / "bracket.3mf",
        model_document(
            mesh_object_xml(1, box_mesh(40.0, 10.0, 10.0)),
            build_item_xml(1, bracket_transform),
        ),
    )

    assembly_components: list[tuple[int, Transform]] = [
        (1, IDENTITY_TRANSFORM),
        (1, rotation_z_translation(90.0, (60.0, 0.0, 0.0))),
    ]
    write_3mf(
        ROOT / "misc" / "assembly.3mf",
        model_document(
            mesh_object_xml(1, cube) + component_object_xml(2, assembly_components),
            build_item_xml(2, None),
        ),
    )

    write_bytes(ROOT / "misc" / "broken.stl", bytes(range(256)))
    write_text(ROOT / "misc" / "notes.txt", NOTES_TEXT)
    write_bytes(ROOT / "misc" / ".hidden" / "secret.stl", cube_stl)
    write_bytes(ROOT / "misc" / "img" / "stale.png", tiny_png_bytes())

    helmet: Path = ROOT / "helmets" / "knight"
    write_bytes(helmet / "shell_top.stl", binary_stl_bytes(spherical_band_mesh(60.0, 24, 0.0, 60.0, 8)))
    write_bytes(helmet / "shell_back.stl", binary_stl_bytes(spherical_band_mesh(60.0, 24, -20.0, 0.0, 4)))
    write_bytes(helmet / "visor.stl", binary_stl_bytes(arc_shell_mesh(62.0, 3.0, 25.0, -60.0, 120.0, 12)))
    write_bytes(helmet / "chin_guard.stl", binary_stl_bytes(box_mesh(90.0, 20.0, 25.0)))
    cheek: Mesh = wedge_mesh(30.0, 40.0, 15.0)
    write_bytes(helmet / "cheek_left.stl", binary_stl_bytes(cheek))
    write_bytes(helmet / "cheek_right.stl", binary_stl_bytes(mirror_x_mesh(cheek)))
    write_text(helmet / "README.txt", HELMET_README_TEXT)
    write_bytes(helmet / "extras" / "strap_clip.stl", binary_stl_bytes(box_mesh(10.0, 10.0, 10.0)))


def main() -> None:
    build_library()
    print(f"wrote {ROOT}")


if __name__ == "__main__":
    main()
