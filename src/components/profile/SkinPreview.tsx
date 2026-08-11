import { useEffect, useRef } from "react";

interface SkinPreviewProps {
  dataUrl: string | null;
  mode: "head" | "body";
  label: string;
  className?: string;
}

export function SkinPreview({ dataUrl, mode, label, className = "" }: SkinPreviewProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const context = canvas.getContext("2d");
    if (!context) return;
    context.imageSmoothingEnabled = false;
    context.clearRect(0, 0, canvas.width, canvas.height);
    if (!dataUrl) return;

    const image = new Image();
    image.onload = () => {
      context.clearRect(0, 0, canvas.width, canvas.height);
      context.imageSmoothingEnabled = false;
      if (mode === "head") {
        context.drawImage(image, 8, 8, 8, 8, 0, 0, canvas.width, canvas.height);
        if (image.height >= 64) context.drawImage(image, 40, 8, 8, 8, 0, 0, canvas.width, canvas.height);
        return;
      }
      const scale = canvas.width / 16;
      const part = (sx: number, sy: number, sw: number, sh: number, dx: number, dy: number) => {
        context.drawImage(image, sx, sy, sw, sh, dx * scale, dy * scale, sw * scale, sh * scale);
      };
      part(8, 8, 8, 8, 4, 0);
      part(20, 20, 8, 12, 4, 8);
      part(44, 20, 4, 12, 0, 8);
      part(36, 52, 4, 12, 12, 8);
      part(4, 20, 4, 12, 4, 20);
      part(20, 52, 4, 12, 8, 20);
      if (image.height >= 64) part(40, 8, 8, 8, 4, 0);
    };
    image.src = dataUrl;
    return () => {
      image.onload = null;
    };
  }, [dataUrl, mode]);

  return (
    <canvas
      ref={canvasRef}
      className={`skin-preview skin-preview--${mode} ${className}`}
      width={mode === "head" ? 128 : 160}
      height={mode === "head" ? 128 : 320}
      role="img"
      aria-label={label}
    />
  );
}
