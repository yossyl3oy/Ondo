using System.Text.Json;
using System.Text.Json.Serialization;
using LibreHardwareMonitor.Hardware;

namespace OndoHardwareMonitor;

class Program
{
    static Computer? _computer;
    static UpdateVisitor? _visitor;

    static void Main(string[] args)
    {
        // Daemon mode: --daemon [interval_ms]
        if (args.Length > 0 && args[0] == "--daemon")
        {
            int intervalMs = 1000;
            if (args.Length > 1 && int.TryParse(args[1], out int parsed))
                intervalMs = Math.Max(100, parsed);

            RunDaemon(intervalMs);
        }
        else
        {
            // Single-shot mode (backward compatible)
            RunOnce();
        }
    }

    static void RunOnce()
    {
        try
        {
            using var computer = CreateComputer();
            computer.Open();
            computer.Accept(new UpdateVisitor());

            var data = ExtractData(computer);
            OutputJson(data);

            computer.Close();
        }
        catch (Exception ex)
        {
            OutputError(ex.Message);
            Environment.Exit(1);
        }
    }

    static void RunDaemon(int intervalMs)
    {
        try
        {
            _computer = CreateComputer();
            _computer.Open();
            _visitor = new UpdateVisitor();

            // Handle graceful shutdown
            Console.CancelKeyPress += (_, e) =>
            {
                e.Cancel = true;
                _computer?.Close();
                Environment.Exit(0);
            };

            // Also handle stdin close (parent process terminated)
            _ = Task.Run(() =>
            {
                try
                {
                    while (Console.Read() != -1) { }
                }
                catch { }
                _computer?.Close();
                Environment.Exit(0);
            });

            while (true)
            {
                try
                {
                    _computer.Accept(_visitor);
                    var data = ExtractData(_computer);
                    OutputJson(data);
                }
                catch (Exception ex)
                {
                    OutputError(ex.Message);
                }

                Thread.Sleep(intervalMs);
            }
        }
        catch (Exception ex)
        {
            OutputError(ex.Message);
            Environment.Exit(1);
        }
    }

    static Computer CreateComputer()
    {
        return new Computer
        {
            IsCpuEnabled = true,
            IsGpuEnabled = true,
            IsMemoryEnabled = true,
            IsMotherboardEnabled = true,
            IsStorageEnabled = true,
            IsControllerEnabled = true
        };
    }

    static void OutputJson(HardwareData data)
    {
        var options = new JsonSerializerOptions
        {
            WriteIndented = false,
            PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
            DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
        };
        Console.WriteLine(JsonSerializer.Serialize(data, options));
    }

    static void OutputError(string message)
    {
        var error = new { error = message };
        var options = new JsonSerializerOptions
        {
            WriteIndented = false,
            PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower
        };
        Console.WriteLine(JsonSerializer.Serialize(error, options));
    }

    static HardwareData ExtractData(Computer computer)
    {
        var data = new HardwareData();

        foreach (var hardware in computer.Hardware)
        {
            switch (hardware.HardwareType)
            {
                case HardwareType.Cpu:
                    data.Cpu = ExtractCpuData(hardware);
                    break;
                case HardwareType.GpuNvidia:
                case HardwareType.GpuAmd:
                case HardwareType.GpuIntel:
                    if (data.Gpu == null) // Only take the first discrete GPU
                        data.Gpu = ExtractGpuData(hardware);
                    break;
                case HardwareType.Motherboard:
                    data.Motherboard = ExtractMotherboardData(hardware);
                    break;
                case HardwareType.Storage:
                    data.Storage ??= new List<StorageData>();
                    var storageData = ExtractStorageData(hardware);
                    if (storageData != null)
                        data.Storage.Add(storageData);
                    break;
            }
        }

        return data;
    }

    static CpuData ExtractCpuData(IHardware hardware)
    {
        var cpu = new CpuData { Name = hardware.Name };
        var coreTemps = new Dictionary<int, float>();
        var coreLoads = new Dictionary<int, float>();

        foreach (var sensor in hardware.Sensors)
        {
            if (sensor.Value == null) continue;
            var value = sensor.Value.Value;

            switch (sensor.SensorType)
            {
                case SensorType.Temperature:
                    if (sensor.Name.Contains("Core #"))
                    {
                        if (int.TryParse(sensor.Name.Replace("Core #", "").Split(' ')[0], out int coreIndex))
                            coreTemps[coreIndex] = value;
                    }
                    else if (sensor.Name.Contains("Package") || sensor.Name.Contains("CPU"))
                    {
                        cpu.Temperature = value;
                    }
                    break;
                case SensorType.Load:
                    if (sensor.Name == "CPU Total")
                    {
                        cpu.Load = value;
                    }
                    else if (sensor.Name.Contains("Core #"))
                    {
                        if (int.TryParse(sensor.Name.Replace("CPU Core #", "").Split(' ')[0], out int coreIndex))
                            coreLoads[coreIndex] = value;
                    }
                    break;
                case SensorType.Clock:
                    if (sensor.Name.Contains("Core #1") || sensor.Name == "CPU Core")
                    {
                        cpu.Frequency = value / 1000f; // MHz to GHz
                    }
                    break;
            }
        }

        // Build core data
        var maxCores = Math.Max(
            coreTemps.Count > 0 ? coreTemps.Keys.Max() + 1 : 0,
            coreLoads.Count > 0 ? coreLoads.Keys.Max() + 1 : 0
        );

        if (maxCores > 0)
        {
            cpu.Cores = new List<CpuCoreData>();
            for (int i = 0; i < maxCores; i++)
            {
                cpu.Cores.Add(new CpuCoreData
                {
                    Index = (uint)i,
                    Temperature = coreTemps.GetValueOrDefault(i, cpu.Temperature),
                    Load = coreLoads.GetValueOrDefault(i, cpu.Load)
                });
            }
        }

        cpu.MaxTemperature = 100f;
        return cpu;
    }

    static GpuData ExtractGpuData(IHardware hardware)
    {
        var gpu = new GpuData { Name = hardware.Name };

        foreach (var sensor in hardware.Sensors)
        {
            if (sensor.Value == null) continue;
            var value = sensor.Value.Value;

            switch (sensor.SensorType)
            {
                case SensorType.Temperature:
                    if (sensor.Name.Contains("GPU Core") || sensor.Name == "GPU")
                        gpu.Temperature = value;
                    break;
                case SensorType.Load:
                    if (sensor.Name == "GPU Core")
                        gpu.Load = value;
                    break;
                case SensorType.Clock:
                    if (sensor.Name == "GPU Core")
                        gpu.Frequency = value / 1000f; // MHz to GHz
                    break;
                case SensorType.SmallData:
                    if (sensor.Name == "GPU Memory Used")
                        gpu.MemoryUsed = value / 1024f; // MB to GB
                    else if (sensor.Name == "GPU Memory Total")
                        gpu.MemoryTotal = value / 1024f; // MB to GB
                    break;
            }
        }

        gpu.MaxTemperature = 95f;
        return gpu;
    }

    static MotherboardData ExtractMotherboardData(IHardware hardware)
    {
        var mb = new MotherboardData { Name = hardware.Name, Fans = new List<FanData>() };

        // Check sub-hardware for sensors (motherboard sensors are usually in sub-hardware)
        foreach (var subHardware in hardware.SubHardware)
        {
            subHardware.Update();
            foreach (var sensor in subHardware.Sensors)
            {
                if (sensor.Value == null) continue;
                var value = sensor.Value.Value;

                switch (sensor.SensorType)
                {
                    case SensorType.Temperature:
                        if (mb.Temperature == 0 && !sensor.Name.Contains("CPU"))
                            mb.Temperature = value;
                        break;
                    case SensorType.Fan:
                        if (value > 0)
                        {
                            mb.Fans.Add(new FanData
                            {
                                Name = sensor.Name,
                                Speed = (uint)value
                            });
                        }
                        break;
                }
            }
        }

        return mb;
    }

    static StorageData? ExtractStorageData(IHardware hardware)
    {
        var storage = new StorageData { Name = hardware.Name };

        foreach (var sensor in hardware.Sensors)
        {
            if (sensor.Value == null) continue;
            var value = sensor.Value.Value;

            switch (sensor.SensorType)
            {
                case SensorType.Temperature:
                    storage.Temperature = value;
                    break;
                case SensorType.Load:
                    if (sensor.Name == "Used Space")
                        storage.UsedPercent = value;
                    break;
                case SensorType.Data:
                    // Data read/written - not used for now
                    break;
            }
        }

        return storage;
    }
}

class UpdateVisitor : IVisitor
{
    public void VisitComputer(IComputer computer)
    {
        computer.Traverse(this);
    }

    public void VisitHardware(IHardware hardware)
    {
        hardware.Update();
        foreach (var subHardware in hardware.SubHardware)
            subHardware.Accept(this);
    }

    public void VisitSensor(ISensor sensor) { }
    public void VisitParameter(IParameter parameter) { }
}

// Data models matching Rust structs
class HardwareData
{
    public CpuData? Cpu { get; set; }
    public GpuData? Gpu { get; set; }
    public List<StorageData>? Storage { get; set; }
    public MotherboardData? Motherboard { get; set; }
}

class CpuData
{
    public string Name { get; set; } = "";
    public float Temperature { get; set; }
    public float MaxTemperature { get; set; }
    public float Load { get; set; }
    public float Frequency { get; set; }
    public List<CpuCoreData>? Cores { get; set; }
}

class CpuCoreData
{
    public uint Index { get; set; }
    public float Temperature { get; set; }
    public float Load { get; set; }
}

class GpuData
{
    public string Name { get; set; } = "";
    public float Temperature { get; set; }
    public float MaxTemperature { get; set; }
    public float Load { get; set; }
    public float Frequency { get; set; }
    public float MemoryUsed { get; set; }
    public float MemoryTotal { get; set; }
}

class StorageData
{
    public string Name { get; set; } = "";
    public float Temperature { get; set; }
    public float UsedPercent { get; set; }
    public float TotalSpace { get; set; }
}

class MotherboardData
{
    public string Name { get; set; } = "";
    public float Temperature { get; set; }
    public List<FanData>? Fans { get; set; }
}

class FanData
{
    public string Name { get; set; } = "";
    public uint Speed { get; set; }
}
