import { memo } from "react";

// The same vector master generates every OS icon and the browser favicon.
export const BrandMark = memo(function BrandMark({
  size = 40,
}: {
  size?: number;
}) {
  return (
    <img
      className="brand-icon"
      src="/brand/app-icon.svg"
      width={size}
      height={size}
      alt=""
      draggable={false}
    />
  );
});
