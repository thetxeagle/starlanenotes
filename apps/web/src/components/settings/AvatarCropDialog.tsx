import { useCallback, useEffect, useRef, useState, type PointerEvent } from "react";
import { Button } from "@/components/ui/button";
import {
  DialogBody,
  DialogCloseAction,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogShell,
  DialogTitle,
} from "@/components/ui/dialog-shell";

const VIEWPORT_SIZE = 280;
const OUTPUT_SIZE = 512;

export function getAvatarDragDelta(delta: number, displayedSize: number): number {
  return delta * VIEWPORT_SIZE / Math.max(1, displayedSize);
}

interface AvatarCropDialogProps {
  source: string;
  onCancel: () => void;
  onConfirm: (croppedImage: string) => void;
}

export function AvatarCropDialog({ source, onCancel, onConfirm }: AvatarCropDialogProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const imageRef = useRef<HTMLImageElement | null>(null);
  const dragRef = useRef<{ x: number; y: number; offsetX: number; offsetY: number } | null>(null);
  const [zoom, setZoom] = useState(1);
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const [ready, setReady] = useState(false);

  const draw = useCallback(() => {
    const image = imageRef.current;
    const canvas = canvasRef.current;
    if (!image || !canvas || !ready) return;
    const context = canvas.getContext("2d");
    if (!context) return;

    const scale =
      Math.max(VIEWPORT_SIZE / image.naturalWidth, VIEWPORT_SIZE / image.naturalHeight) * zoom;
    const renderedWidth = image.naturalWidth * scale;
    const renderedHeight = image.naturalHeight * scale;
    const cropWidth = VIEWPORT_SIZE / scale;
    const cropHeight = VIEWPORT_SIZE / scale;
    const sourceX = Math.min(
      Math.max(0, (renderedWidth - VIEWPORT_SIZE) / 2 - offset.x),
      (image.naturalWidth - cropWidth) * scale,
    );
    const sourceY = Math.min(
      Math.max(0, (renderedHeight - VIEWPORT_SIZE) / 2 - offset.y),
      (image.naturalHeight - cropHeight) * scale,
    );

    context.clearRect(0, 0, OUTPUT_SIZE, OUTPUT_SIZE);
    context.drawImage(
      image,
      sourceX / scale,
      sourceY / scale,
      cropWidth,
      cropHeight,
      0,
      0,
      OUTPUT_SIZE,
      OUTPUT_SIZE,
    );
  }, [offset, ready, zoom]);

  useEffect(() => {
    const image = new Image();
    image.onload = () => {
      imageRef.current = image;
      setReady(true);
    };
    image.src = source;
    return () => {
      image.onload = null;
    };
  }, [source]);

  useEffect(() => {
    draw();
  }, [draw]);

  const handlePointerDown = (event: PointerEvent<HTMLCanvasElement>) => {
    event.currentTarget.setPointerCapture(event.pointerId);
    dragRef.current = { x: event.clientX, y: event.clientY, offsetX: offset.x, offsetY: offset.y };
  };

  const handlePointerMove = (event: PointerEvent<HTMLCanvasElement>) => {
    const drag = dragRef.current;
    if (!drag) return;
    const { width, height } = event.currentTarget.getBoundingClientRect();
    setOffset({
      x: drag.offsetX + getAvatarDragDelta(event.clientX - drag.x, width),
      y: drag.offsetY + getAvatarDragDelta(event.clientY - drag.y, height),
    });
  };

  const handlePointerUp = () => {
    dragRef.current = null;
  };

  const handleConfirm = () => {
    const canvas = canvasRef.current;
    if (!canvas || !ready) return;
    onConfirm(canvas.toDataURL("image/jpeg", 0.88));
  };

  return (
    <DialogShell
      labelledBy="avatar-crop-title"
      describedBy="avatar-crop-description"
      maxWidth="sm"
      onClose={onCancel}
    >
      <DialogHeader>
        <div>
          <DialogTitle id="avatar-crop-title">Adjust profile picture</DialogTitle>
          <DialogDescription id="avatar-crop-description">
            Drag to reposition, then choose a zoom.
          </DialogDescription>
        </div>
        <DialogCloseAction label="Close profile picture editor" onClick={onCancel} />
      </DialogHeader>
      <DialogBody>
        <div
          className="bg-bg border-border rounded-card mx-auto overflow-hidden border"
          style={{ width: "100%", maxWidth: VIEWPORT_SIZE, aspectRatio: "1 / 1" }}
        >
          <canvas
            ref={canvasRef}
            width={OUTPUT_SIZE}
            height={OUTPUT_SIZE}
            className="h-full w-full cursor-grab touch-none active:cursor-grabbing"
            onPointerDown={handlePointerDown}
            onPointerMove={handlePointerMove}
            onPointerUp={handlePointerUp}
            onPointerCancel={handlePointerUp}
          />
        </div>
        <label className="text-text-muted mt-4 block text-xs">
          Zoom
          <input
            type="range"
            min="1"
            max="3"
            step="0.05"
            value={zoom}
            onChange={(event) => setZoom(Number(event.target.value))}
            className="accent-primary mt-2 w-full"
            aria-label="Avatar zoom"
          />
        </label>
      </DialogBody>
      <DialogFooter>
        <Button type="button" variant="secondary" onClick={onCancel}>
          Cancel
        </Button>
        <Button type="button" disabled={!ready} onClick={handleConfirm}>
          Use picture
        </Button>
      </DialogFooter>
    </DialogShell>
  );
}
