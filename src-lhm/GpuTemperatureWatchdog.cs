namespace OndoHardwareMonitor;

/// Decides when the daemon should rebuild its GPU devices because the shown
/// GPU stopped reporting a temperature.
///
/// LHM creates a GPU's sensors once, when it creates the device object, and
/// never retries. A daemon that had been running for a day was seen reporting
/// no NVIDIA "GPU Core" temperature while a fresh process on the same machine
/// read it fine (load and clocks kept working), so rebuilding the devices is
/// the only way to get it back without restarting the daemon.
///
/// Some GPUs never report a temperature, so the rebuilds back off and stop
/// after <see cref="MaxResets"/> until a temperature shows up again.
class GpuTemperatureWatchdog
{
    /// Consecutive ticks without a temperature before the first rebuild; each
    /// further rebuild waits twice as long.
    public const int FirstResetAfterTicks = 10;

    public const int MaxResets = 6;

    int _missingTicks;

    /// Rebuilds done since the GPU last reported a temperature.
    public int Resets { get; private set; }

    /// Whether the GPU reported a temperature at any point. A rebuild for
    /// one that never did is likely a GPU without the sensor, not a fault.
    public bool HadTemperature { get; private set; }

    /// Records one daemon tick. Returns true when the GPU devices should be
    /// rebuilt now.
    public bool Observe(bool gpuPresent, bool hasTemperature)
    {
        if (hasTemperature)
        {
            _missingTicks = 0;
            Resets = 0;
            HadTemperature = true;
            return false;
        }

        // No GPU at all (e.g. while the driver restarts) isn't a missing
        // sensor; LHM re-adds the device by itself when it comes back.
        if (!gpuPresent)
        {
            _missingTicks = 0;
            return false;
        }

        if (Resets >= MaxResets)
            return false;

        if (++_missingTicks < FirstResetAfterTicks << Resets)
            return false;

        _missingTicks = 0;
        Resets++;
        return true;
    }
}
