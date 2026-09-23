using System.Net;

namespace AcDream.Plugins.MossTank.Tests;

/// <summary>
/// The HTTP transport against a handler that answers from the test: every
/// way the service can fail comes back as a reason, never as an exception.
/// </summary>
public sealed class HttpVtankGameInfoTransportTests
{
    private static readonly Uri Address = new(VtankGameInfoUpdater.SourceAddress);

    [Fact]
    public async Task AnAnswerIsReadWhole()
    {
        using var transport = new HttpVtankGameInfoTransport(
            new Handler((_, _) => Task.FromResult(Text("1\r\nDBVersion\r\n"))));

        VtankGameInfoAnswer answer = await transport.GetAsync(Address, CancellationToken.None);

        Assert.Equal(new VtankGameInfoAnswer(true, "1\r\nDBVersion\r\n"), answer);
    }

    /// <summary>Mutation: accept any status, and the error page reads as an answer.</summary>
    [Fact]
    public async Task AnErrorStatusIsAReason()
    {
        using var transport = new HttpVtankGameInfoTransport(new Handler((_, _) =>
            Task.FromResult(new HttpResponseMessage(HttpStatusCode.ServiceUnavailable)
            {
                ReasonPhrase = "Service Unavailable",
                Content = new StringContent("<html>down</html>"),
            })));

        VtankGameInfoAnswer answer = await transport.GetAsync(Address, CancellationToken.None);

        Assert.Equal(new VtankGameInfoAnswer(false, "HTTP 503 Service Unavailable"), answer);
    }

    /// <summary>Mutation: let the request's exception through, and the check faults.</summary>
    [Fact]
    public async Task AFailedRequestIsAReason()
    {
        using var transport = new HttpVtankGameInfoTransport(new Handler((_, _) =>
            throw new HttpRequestException("No such host is known.")));

        VtankGameInfoAnswer answer = await transport.GetAsync(Address, CancellationToken.None);

        Assert.Equal(new VtankGameInfoAnswer(false, "No such host is known."), answer);
    }

    /// <summary>
    /// A service that does not answer in time is given up on with a reason.
    /// Mutation: drop the timeout's catch, and the wait ends in an exception.
    /// </summary>
    [Fact]
    public async Task ASilentServiceTimesOut()
    {
        using var transport = new HttpVtankGameInfoTransport(
            new Handler(static async (_, cancellation) =>
            {
                await Task.Delay(Timeout.Infinite, cancellation);
                return Text("never");
            }),
            timeout: TimeSpan.FromMilliseconds(50));

        VtankGameInfoAnswer answer = await transport.GetAsync(Address, CancellationToken.None);

        Assert.Equal(new VtankGameInfoAnswer(false, "no answer within 0.05 seconds"), answer);
    }

    /// <summary>
    /// An answer larger than the cap is refused rather than read into memory.
    /// Mutation: leave the cap off the client, and the oversized answer is
    /// taken.
    /// </summary>
    [Fact]
    public async Task AnOversizedAnswerIsRefused()
    {
        using var transport = new HttpVtankGameInfoTransport(
            new Handler((_, _) => Task.FromResult(Text(new string('x', 4096)))),
            maximumAnswerBytes: 1024);

        VtankGameInfoAnswer answer = await transport.GetAsync(Address, CancellationToken.None);

        Assert.False(answer.Succeeded);
    }

    private static HttpResponseMessage Text(string text) =>
        new(HttpStatusCode.OK) { Content = new StringContent(text) };

    private sealed class Handler(
        Func<HttpRequestMessage, CancellationToken, Task<HttpResponseMessage>> answer)
        : HttpMessageHandler
    {
        protected override Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request,
            CancellationToken cancellationToken) =>
            answer(request, cancellationToken);
    }
}
