# Draws the extension icons from code, so the artwork stays in the repository as
# a script instead of an unexplained blob:
#
#   powershell -ExecutionPolicy Bypass -File icons/make-icons.ps1
#
# The mark: dark rounded tile with an accent border and a double chevron, the
# "jump forward" glyph the tool is about.

Add-Type -AssemblyName System.Drawing

$sizes = 16, 32, 48, 128
$here = Split-Path -Parent $MyInvocation.MyCommand.Path

$tile = [System.Drawing.Color]::FromArgb(255, 0x14, 0x16, 0x1a)
$accent = [System.Drawing.Color]::FromArgb(255, 0x58, 0xa6, 0xff)

foreach ($size in $sizes) {
    $bitmap = [System.Drawing.Bitmap]::new($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.Clear([System.Drawing.Color]::Transparent)

    $scale = $size / 128.0
    $inset = [Math]::Max(0.5, 2.0 * $scale)
    $edge = $size - 2 * $inset
    $radius = [Math]::Max(1.0, 26.0 * $scale)
    $diameter = $radius * 2
    $rect = [System.Drawing.RectangleF]::new($inset, $inset, $edge, $edge)

    $path = [System.Drawing.Drawing2D.GraphicsPath]::new()
    $path.AddArc($rect.X, $rect.Y, $diameter, $diameter, 180, 90)
    $path.AddArc($rect.Right - $diameter, $rect.Y, $diameter, $diameter, 270, 90)
    $path.AddArc($rect.Right - $diameter, $rect.Bottom - $diameter, $diameter, $diameter, 0, 90)
    $path.AddArc($rect.X, $rect.Bottom - $diameter, $diameter, $diameter, 90, 90)
    $path.CloseFigure()

    $tileBrush = [System.Drawing.SolidBrush]::new($tile)
    $graphics.FillPath($tileBrush, $path)
    $borderPen = [System.Drawing.Pen]::new($accent, [Math]::Max(1.0, 3.0 * $scale))
    $graphics.DrawPath($borderPen, $path)

    $accentBrush = [System.Drawing.SolidBrush]::new($accent)
    foreach ($shift in 0.0, 0.26) {
        [System.Drawing.PointF[]] $points = @(
            [System.Drawing.PointF]::new((0.24 + $shift) * $size, 0.30 * $size),
            [System.Drawing.PointF]::new((0.50 + $shift) * $size, 0.50 * $size),
            [System.Drawing.PointF]::new((0.24 + $shift) * $size, 0.70 * $size)
        )
        $graphics.FillPolygon($accentBrush, $points)
    }

    $graphics.Dispose()
    $target = Join-Path $here ('icon' + $size + '.png')
    $bitmap.Save($target, [System.Drawing.Imaging.ImageFormat]::Png)
    $bitmap.Dispose()
    Write-Output ('wrote ' + $target)
}
