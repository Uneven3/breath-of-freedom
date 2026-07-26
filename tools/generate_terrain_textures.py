import math
import os
import random
import struct
import zlib

def write_png(filepath, width, height, pixels):
    """Write an RGBA image as a standard PNG file using pure Python zlib."""
    raw_data = bytearray()
    for y in range(height):
        raw_data.append(0) # filter type 0 (None)
        for x in range(width):
            r, g, b, a = pixels[y * width + x]
            raw_data.extend([r, g, b, a])

    compressed = zlib.compress(bytes(raw_data), 9)

    png_bytes = bytearray(b'\x89PNG\r\n\x1a\n')

    # IHDR chunk
    ihdr_data = struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0)
    ihdr_crc = zlib.crc32(b'IHDR' + ihdr_data)
    png_bytes.extend(struct.pack('>I', len(ihdr_data)))
    png_bytes.extend(b'IHDR')
    png_bytes.extend(ihdr_data)
    png_bytes.extend(struct.pack('>I', ihdr_crc))

    # IDAT chunk
    idat_crc = zlib.crc32(b'IDAT' + compressed)
    png_bytes.extend(struct.pack('>I', len(compressed)))
    png_bytes.extend(b'IDAT')
    png_bytes.extend(compressed)
    png_bytes.extend(struct.pack('>I', idat_crc))

    # IEND chunk
    iend_crc = zlib.crc32(b'IEND')
    png_bytes.extend(struct.pack('>I', 0))
    png_bytes.extend(b'IEND')
    png_bytes.extend(struct.pack('>I', iend_crc))

    os.makedirs(os.path.dirname(filepath), exist_ok=True)
    with open(filepath, 'wb') as f:
        f.write(png_bytes)
    print(f"Generated texture: {filepath}")

def generate_grass_texture(width=512, height=512):
    pixels = []
    random.seed(42)
    for y in range(height):
        # Seamless torus UV wrapping for 100% seamless tileable texture
        ny = (y / float(height)) * 2.0 * math.pi
        sin_y = math.sin(ny)
        cos_y = math.cos(ny)
        
        for x in range(width):
            nx = (x / float(width)) * 2.0 * math.pi
            sin_x = math.sin(nx)
            cos_x = math.cos(nx)
            
            # Seamless 4D torus noise combination
            wave1 = (sin_x * cos_y + sin_y * cos_x) * 0.025
            wave2 = (math.sin(nx * 2.0) * math.cos(ny * 2.0)) * 0.015
            
            total_noise = wave1 + wave2
            
            r = int(clamp((0.27 + total_noise) * 255))
            g = int(clamp((0.50 + total_noise * 1.2) * 255))
            b = int(clamp((0.22 + total_noise * 0.7) * 255))
            pixels.append((r, g, b, 255))
    return pixels

def generate_dirt_texture(width=256, height=256):
    pixels = []
    random.seed(101)
    for y in range(height):
        for x in range(width):
            # Rich dark soil base with soil speckles
            noise1 = math.sin(x * 0.15) * math.sin(y * 0.15) * 0.05
            noise2 = (random.random() - 0.5) * 0.12
            
            r = int(clamp((0.30 + noise1 + noise2) * 255))
            g = int(clamp((0.22 + noise1 * 0.8 + noise2 * 0.8) * 255))
            b = int(clamp((0.14 + noise1 * 0.6 + noise2 * 0.5) * 255))
            pixels.append((r, g, b, 255))
    return pixels

def generate_path_texture(width=256, height=256):
    pixels = []
    random.seed(202)
    for y in range(height):
        for x in range(width):
            # Lighter worn dirt path texture
            noise1 = math.cos(x * 0.08) * math.sin(y * 0.08) * 0.06
            noise2 = (random.random() - 0.5) * 0.08
            
            r = int(clamp((0.48 + noise1 + noise2) * 255))
            g = int(clamp((0.38 + noise1 * 0.9 + noise2) * 255))
            b = int(clamp((0.26 + noise1 * 0.7 + noise2 * 0.8) * 255))
            pixels.append((r, g, b, 255))
    return pixels

def generate_leaves_texture(width=256, height=256):
    pixels = []
    random.seed(303)
    for y in range(height):
        for x in range(width):
            # Forest floor leaf litter (brownish autumn shades + organic spots)
            spot = math.sin(x * 0.25) * math.cos(y * 0.25)
            noise2 = (random.random() - 0.5) * 0.15
            
            r = int(clamp((0.40 + spot * 0.08 + noise2) * 255))
            g = int(clamp((0.26 + spot * 0.06 + noise2 * 0.7) * 255))
            b = int(clamp((0.14 + spot * 0.04 + noise2 * 0.4) * 255))
            pixels.append((r, g, b, 255))
    return pixels

def clamp(val, min_val=0, max_val=255):
    return max(min_val, min(max_val, val))

def generate_grass_card_texture(width=512, height=512):
    """Generate a 512x512 RGBA texture of dense 2D foliage wall with overlapping blades and cutout alpha."""
    pixels = [(0, 0, 0, 0)] * (width * height)
    random.seed(777)
    
    # 36 dense overlapping blade parameters: (base_x_ratio, tip_x_ratio, height_ratio, width_ratio)
    blades = []
    for i in range(36):
        base_x = 0.02 + (i / 35.0) * 0.96 + (random.random() - 0.5) * 0.03
        tip_x = base_x + (random.random() - 0.5) * 0.16
        h_ratio = 0.55 + random.random() * 0.42
        w_ratio = 0.035 + random.random() * 0.025
        blades.append((base_x, tip_x, h_ratio, w_ratio))
    
    for y in range(height):
        # norm_y: 0.0 at bottom (y=511, roots), 1.0 at top (y=0, tips)
        norm_y = (height - 1 - y) / float(height)
        
        for x in range(width):
            norm_x = x / float(width)
            
            best_alpha = 0
            best_color = (0, 0, 0, 0)
            
            for base_x, tip_x, h_ratio, w_ratio in blades:
                if norm_y > h_ratio:
                    continue
                
                # Taper from base to tip
                progress = norm_y / h_ratio
                center_x = base_x + (tip_x - base_x) * (progress ** 0.85)
                curr_w = w_ratio * (1.0 - progress * 0.80)
                
                dist_x = abs(norm_x - center_x)
                if dist_x <= curr_w:
                    # Inside blade with sharp crisp cutout edge
                    edge_fade = 1.0 - (dist_x / curr_w)
                    alpha = 255 if edge_fade > 0.15 else 0
                    
                    # Color gradient: dark root -> lush mid -> sunny tip
                    if progress < 0.25:
                        t = progress / 0.25
                        r = 0.14 + t * (0.27 - 0.14)
                        g = 0.28 + t * (0.50 - 0.28)
                        b = 0.11 + t * (0.22 - 0.11)
                    else:
                        t = (progress - 0.25) / 0.75
                        r = 0.27 + t * (0.36 - 0.27)
                        g = 0.50 + t * (0.62 - 0.50)
                        b = 0.22 + t * (0.25 - 0.22)
                    
                    color = (int(r * 255), int(g * 255), int(b * 255), alpha)
                    if alpha > best_alpha:
                        best_alpha = alpha
                        best_color = color
            
            pixels[y * width + x] = best_color
    return pixels

def main():
    target_dir = "assets/textures/terrain"
    props_dir = "assets/textures/props"
    write_png(os.path.join(target_dir, "T_GroundGrass_Albedo.png"), 512, 512, generate_grass_texture())
    write_png(os.path.join(target_dir, "T_GroundDirt_Albedo.png"), 256, 256, generate_dirt_texture())
    write_png(os.path.join(target_dir, "T_GroundPath_Albedo.png"), 256, 256, generate_path_texture())
    write_png(os.path.join(target_dir, "T_GroundLeaves_Albedo.png"), 256, 256, generate_leaves_texture())
    write_png(os.path.join(props_dir, "T_GrassCard_Albedo.png"), 512, 512, generate_grass_card_texture())

if __name__ == "__main__":
    main()
