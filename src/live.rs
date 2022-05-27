use std::{
    pin::Pin,
    str::FromStr,
    task::{Context, Poll},
};

use futures::{ready, Future, FutureExt, SinkExt, Stream};
use rand::{prelude::SliceRandom, rngs::OsRng};
use serde::Deserialize;
use thiserror::Error;
use time::{Duration, OffsetDateTime};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::stream::{CreateStream, DisposableResult, DisposableStream, DurableStream};

#[derive(Debug, Error)]
pub enum Error {
    #[error("error decoding message")]
    DecodingError,
    #[error("decode json failed")]
    JsonError(#[from] serde_json::Error),
    #[error("{0}")]
    TransportError(#[from] tokio_tungstenite::tungstenite::Error),
}

const WS_SERVERS: &[&str] = &[
    "wss://ws1.blitzortung.org",
    "wss://ws7.blitzortung.org",
    "wss://ws8.blitzortung.org",
];

fn decode(ciphertext: &str) -> String {
    // obfuscated JS source:
    // var a,
    // e = {
    // },
    // d = b.split(''),
    // c = d[0],
    // f = c,
    // g = [
    //   c
    // ],
    // h = 256;
    // o = h;
    // for (b = 1; b < d.length; b++) {
    //	a = d[b].charCodeAt(0);
    // 	a = h > a ? d[b] : e[a] ? e[a] : f + c;
    // 	g.push(a);
    // 	c = a.charAt(0);
    // 	e[o] = f + c;
    // 	o++;
    // 	f = a;
    // }
    // return g.join('')

    let mut chars = ciphertext.chars();

    let mut c = chars.next().unwrap();
    let mut prev = c.to_string();
    let mut out = c.to_string();
    let mut dict = Vec::<String>::with_capacity(ciphertext.chars().count());

    // this could probably be written a little more elegantly...
    for char in chars {
        let code = char as u32;

        let a = if 256 > code {
            char.to_string()
        } else if let Some(stored) = dict.get(code as usize - 256) {
            stored.clone()
        } else {
            format!("{prev}{c}")
        };
        out.push_str(&a);
        c = a.chars().next().unwrap();
        dict.push(format!("{prev}{c}"));
        prev = a;
    }

    out
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Station {
    pub sta: u32,
    pub time: i64,
    pub lat: f32,
    pub lon: f32,
    pub alt: i32,
    pub status: u32,
}

mod unix_nanos_serde {
    use serde::{de, Deserialize, Deserializer};
    use time::OffsetDateTime;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let nanos = i128::deserialize(deserializer)?;
        OffsetDateTime::from_unix_timestamp_nanos(nanos).map_err(de::Error::custom)
    }
}

mod duration_nanos_serde {
    use serde::{Deserialize, Deserializer};
    use time::Duration;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        i64::deserialize(deserializer).map(Duration::nanoseconds)
    }
}

mod duration_secs_serde {
    use serde::{Deserialize, Deserializer};
    use time::Duration;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        f64::deserialize(deserializer).map(Duration::seconds_f64)
    }
}

#[derive(Debug, Deserialize)]
pub struct Strike {
    #[serde(with = "unix_nanos_serde")]
    pub time: OffsetDateTime,
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
    pub pol: i32,
    /// Maximum deviation span.
    #[serde(with = "duration_nanos_serde")]
    pub mds: Duration,
    /// Minimum cycle gap (degrees).
    pub mcg: f32,
    pub status: i32,
    pub region: u8,
    pub sig: Vec<Station>,
    #[serde(with = "duration_secs_serde")]
    pub delay: Duration,
}

impl FromStr for Strike {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let json = decode(s);
        serde_json::from_str(&json).map_err(Into::into)
    }
}

async fn connect() -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, Error> {
    let server = *WS_SERVERS.choose(&mut OsRng).unwrap();
    let (mut stream, _) = connect_async(server).await?;

    stream.send(Message::Text("{\"a\": 542}".into())).await?; // start receiving

    Ok(stream)
}

impl DisposableStream for WebSocketStream<MaybeTlsStream<TcpStream>> {
    type Item = Result<Strike, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<DisposableResult<Self::Item>> {
        let message = match ready!(Stream::poll_next(self, cx)) {
            Some(Ok(message)) => message,
            Some(Err(e)) => return Poll::Ready(DisposableResult::err(e.into())),
            None => return Poll::Ready(DisposableResult::Discard), // discard the stream if it has ended
        };

        match message {
            Message::Text(s) => Poll::Ready(DisposableResult::Some(s.parse())),
            Message::Close(_) => Poll::Ready(DisposableResult::Discard),
            _ => Poll::Pending,
        }
    }
}

pub struct StreamCreator;

impl CreateStream for StreamCreator {
    type Stream = WebSocketStream<MaybeTlsStream<TcpStream>>;
    type Error = Error;

    fn connect() -> Pin<Box<dyn Future<Output = Result<Self::Stream, Self::Error>>>> {
        connect().boxed()
    }
}

#[must_use]
pub fn create_stream() -> DurableStream<StreamCreator> {
    DurableStream::connect()
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;

    use super::{create_stream, decode};

    #[tokio::test]
    async fn ten_strikes() {
        let mut stream = create_stream().take(10);

        while let Some(result) = stream.next().await {
            result.unwrap().unwrap();
        }
    }

    #[test]
    fn decode_simple() {
        let decoded = decode("{\"time\":165360ċ02čČ950ĕ,\"latĆ38.7Ċ126ėlonĆĞ932Ġ1ėalě:0ėpolĆĵ\"mdsĆ14728ėmcgŀ97ėstĚuĿĴėregiħŀōiŉ:[ĀŎaĆēįāăą:43181ďĥĚĆ45.ńī6ŅĘř:21.ī396Ĥ\"ıĳģ4ōŏtőŀ0},ŠŏĆ235ŹĂĄŲ3ċŮŌĘű:Ĕŵ157ĠĥŻĢ.1ī2ƂİĲĆ6īƊŐŒĢƐƒŢżĠƟƙŨŪ6408Ɵęĳ42ſǅĐ3ƩĨż3.ĊġŹƆĩƯƶƌƸ2ƺ\"šƎ59ėǀƛ80ƥ2Űǉ7.ƃǃūǐŊǯĔ6ĭƱĳĢǤǠƋƍćƏƑǽƼńǄǥŧǧƃ25ǬĜǮĝč7ǼĦǑ-6.ǩǅǆǹƔĻšǜŀǞȂǡȀǸŦƚũǊƭ5ƄǈĆƣďĈųǳć4.41Ů8ƉƅƲǒǼȟǿŮǟȤƯƕȇȨ4Ū17ŸȍũǮĐŃȭŻĐȷ8Ɯ4Ǽǘ:ƵȃȠćȢƻĩ9ƘȈȩĉĝǫƠĳƣ348ĉɔǑȶ2Ū9ȳȽǺĝǛǿɢȃƔȹǏȧǁɰ06Ůɏƣ6ĝ3ʇȴŴ82ʓ6ƟɜūƄɁǝɄƓƽ4ƄǦȩŭēȭơɰƬƜʎťȔĆ-ž5ǂȌɺŲȼʚŲʜȄʇƿɧɊɘǃʥɭ0.ƁʈȒȴȶǊ90ʖȾ1ɯɽʛȣʝƯȬɈʅ9Ģɚʊ˂Ǩ5ˎŹʬ:ǮȬ37țʳɡťʶɡʸƳ7ȼʡų0Ǆċ˛ğ7ʫŻȗʎ7ċɛȾʓʵǾƸȁɣżĉ˰ʼ˞0Ȭˀȯ˂5̑ŋǇƪǓĈʓ˧ɜƗʃ˫ƹ˒Ƽɥɦɉ5ģʎʲȮũ9ſ8ɥȸȴƥǯƥĉˌƇ˞ːƎ˭ż˞̢ǁ̑ĕȆɬ̏˃ďḑ̌ŻǓʇĊ˳Ȝ:ʌ̷ˬ̟ŊɼʄŲ̑˲ˠơƣɸɍƃ̯ž09ƕʈ͌ĭ˪̃ȡ̹ȋ˚͔ũ̑ȒƖɏʟ.ɱ˽ʕȴŽǔʎǰ͌ŋ͏̞̆Ł̀˱Ʀđ͘ˁğȸʍˇͳʎɚ˿ĳ8ʲ̝ɿȤ̬̣̉˘ʰɫ̨ȰŮĈɳŀȗɶł˯͌˾;̅ʀ̇ǅ˖͕͡ʕȼ̨ȸǯēǩȓɕȶ˽˘̙ˍ˯ʙͧ͐΀̬Ǽ΃ēȬȓʦĞɍ˥ͶźǑƭƬǄģťʗƴ;ΔʝŁĉέͭƂĈǇ͙˂ɥ˦Ȧˡ˻ƥʇǗˍȹϗ̹ĢƁϜųɥƭΛϡͳŮʟʃˡȶΙϣ͌ĔΨϮǪϱČƗǩʊ͟ǩƧʐǌĈˋ͌ɹ˫ϘȄČť˱ʌɰł˶ȻʎƜ΋Ήɶʲɜ˘͏4ͩȋʠʼĉŬŽЈŵ9ˊƦȴ̪łǊ˥͌ŬͦƷʷ͑ć̼ŲƃŬЀ́:˥ȷčЭώƔϼŬ8ɱиĬ;ͩ˘Щɉʕʇύ̨ц˴ͥͷьюɸѐ̜πЦнɶˊЄ̡ȋͱǋųʒůъżϼ˥ƞАɌХ̹аΗǁ˸ɌЋф˞ч˦˕ѱɑǅȹϪɻѣлż̹ɱѕѼʓ̑ʫ͙žʎ̯ͪǋјύɜŮȞπГţĢϱŃ̻α϶ɰˎ̯͡ŴƂƯϔȾĔɀҠϮƦпɐʰΞЛƴ̛̯ι̍ɘΦϬɟǿѥ΀сҤʈƴĻΜҖʕɸϺƪǋċų҄ҝʎѸнϸτʼΑŁĊЮʌɯ͋ѱŴȬŮͽ˨ѷӄŒҡоǲͬɰ7ˋŬͱ̖Ģ˸ӑȕǮʟ̌ɇ˨ʽә̆ɶΥӱǆųщβ̪͠ϳ͆Ǒɖĭɘ˵˨ӳҟҌӆΪǻɫ˱ǆƧ̀Ӎ͹łяѱˈȸғ͌οҌӮЎҐŲǆɥ̤Ю˘ɯӁѱ̪Ɩ˸Ҝ̀ёӬʷ]ėdeęyƳƬ}");
        assert_eq!(decoded, "{\"time\":1653603602036095000,\"lat\":38.753126,\"lon\":8.932751,\"alt\":0,\"pol\":0,\"mds\":14728,\"mcg\":197,\"status\":0,\"region\":1,\"sig\":[{\"sta\":951,\"time\":4318102,\"lat\":45.289368,\"lon\":21.933966,\"alt\":264,\"status\":10},{\"sta\":2358,\"time\":4336107,\"lat\":50.215775,\"lon\":12.193296,\"alt\":693,\"status\":12},{\"sta\":2757,\"time\":4364087,\"lat\":42.908203,\"lon\":23.535318,\"alt\":829,\"status\":12},{\"sta\":1059,\"time\":4380152,\"lat\":47.666431,\"lon\":19.650627,\"alt\":129,\"status\":10},{\"sta\":2840,\"time\":4386625,\"lat\":37.380379,\"lon\":-6.010887,\"alt\":20,\"status\":12},{\"sta\":1027,\"time\":4421956,\"lat\":50.021645,\"lon\":14.411084,\"alt\":239,\"status\":10},{\"sta\":2923,\"time\":4431768,\"lat\":47.20726,\"lon\":20.483349,\"alt\":93,\"status\":12},{\"sta\":898,\"time\":4465382,\"lat\":50.348656,\"lon\":4.243945,\"alt\":138,\"status\":2},{\"sta\":2113,\"time\":4480610,\"lat\":50.638306,\"lon\":5.822267,\"alt\":316,\"status\":12},{\"sta\":2746,\"time\":4481956,\"lat\":48.133301,\"lon\":-1.54365,\"alt\":44,\"status\":4},{\"sta\":2067,\"time\":4483646,\"lat\":50.396179,\"lon\":4.42907,\"alt\":134,\"status\":12},{\"sta\":2956,\"time\":4491249,\"lat\":50.805138,\"lon\":7.563787,\"alt\":121,\"status\":12},{\"sta\":674,\"time\":4504036,\"lat\":50.771,\"lon\":6.307369,\"alt\":224,\"status\":10},{\"sta\":2654,\"time\":4510566,\"lat\":50.555977,\"lon\":13.162287,\"alt\":583,\"status\":12},{\"sta\":988,\"time\":4526305,\"lat\":49.989841,\"lon\":15.615657,\"alt\":251,\"status\":10},{\"sta\":2518,\"time\":4550040,\"lat\":50.30202005on\":3.065304,\"alt\":63,\"status\":12},{\"sta\":1938,\"time\":4554508,\"lat\":50.947666,\"lon\":11.092361,\"alt\":271,\"status\":12},{\"sta\":2549,\"time\":4557935,\"lat\":46.867367,\"lon\":21.530666,\"alt\":97,\"status\":12},{\"sta\":14lt\":2time\":4570368,\"lat\":50.741383,\"lon\":4.830499,\"alt\":85,\"status\":12},{\"sta\":89me\":454,\"ti91542,\"lat\":50.010166,\"lon\":16.244774,\"alt\":369;0},{\"sta\":2608,\"time\":4592674,\"lat\":41.695019,\"lon\":24.739187,\"alt\":1746,\"status\":12},{\"sta\":1899,\"time\":4595569,\"lat\":48.763767,\"lon\":19.140261,\"alt\":369;2},{\"sta\":1465,\"time\":4596167,\"lat\":50.987827,\"lon\":6.315068,\"alt\":111;2},{\"sta\":1239,\"time\":4598192,\"lat\":50.810463,\"lon\":4.915987,\"alt\":50;0},{\"sta\":1152,\"time\":4605801,\"lat\":51.00177,\"lon\":5.901607,\"alt\":45,\"status\":2},{\"sta\":2601,\"time\":4634847,\"lat\":50.843033,\"lon\":4.413245,\"alt\":91,\"status\":4},{\"sta\":2256,\"time\":4651821,\"lat\":51.299057,\"lon\":9.474237,\"alt\":181,\"status\":4},{\"sta\":18,\"time\":4661850;224:37.40321,\"lon\":24.918886,\"alt\":132;},{\"sta\":2916,\"time\":4670667,\"lat\":37.403271,\"lon\":24.918894,\"alt\":133,\"status\":4},{\"sta\":2490,\"time\":4698825,\"lat\":42.458202,\"lon\":24.937107,\"alt\":417,\"status\":4},{\"sta\":99me\":454e\":4711777,224:51.407856,\"lon\":7.208118,\"alt\":133,\"status\":2},{\"sta\":866,\"time\":4722551,\"lat\":51.30,\"lon\":12542.06667,\"alt\":100,\"status\":2},{\"sta\":912,\"time\":4725184,\"lat\":50.4813,\"lon\":1925.96291,\"alt\":509,\"status\":2},{\"sta\":1578,\"time\":4754101,\"lat\":50.869583,\"lon\":14.756683,\"alt\":311;\"status\":4},{\"sta\":1661,\"time\":4761690,\"lat\":51.367943,\"lon\":12.364556,\"alt\":130,\"status\":4},{\"sta\":1049,\"time\":4851453,\"lat\":51.633404,\"lon\":5.561097,\"alt\":17,\"status\":2},{\"sta\":1831,\"time\":4870718,\"lat\":43.112713,\"lon\":-7.460523,\"alt\":448,\"status\":4},{\"sta\":2474,\"time\":4874521,\"lat\":49.0998105on\":20.278336,\"alt\":700,\"status\":4},{\"sta\":1292,\"time\":48777lt\":2lat\":51.534786,\"lon\":4.441551,\"alt\":6,\"status\":2},{\"sta\":1606,\"time\":4879852,\"lat\":51.913483,\"lon\":9.357167,\"alt\":232;\"status\":4}],\"delay\":6.1}");
    }
}
