using Microsoft.AspNetCore.SignalR;

namespace InterviewDemoApp.Web.Hubs;

public class NumberHub: Hub<INumberClientHub>
{
    public async Task SetupIntervals(int interval)
    {
        await Clients.Caller.SendMessage($"Setup intervals for {interval} seconds.");
    }

    public async Task AddNumber(ulong number)
    {
        await Clients.Caller.SendMessage("Error - You need to call SetupIntervals first!");
    }

    public Task Halt()
    {
        return Task.CompletedTask;
    }

    public Task Resume()
    {
        return Task.CompletedTask;
    }

    public Task Quit()
    {
        return Task.CompletedTask;
    }
}