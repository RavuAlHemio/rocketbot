<?php

require_once __DIR__ . '/vendor/autoload.php';

use Wikimedia\Parsoid\Parsoid;
use Wikimedia\Parsoid\Config\PageConfig;
use Wikimedia\Parsoid\Config\PageContent;
use Wikimedia\Parsoid\Config\SiteConfig;
use Wikimedia\Parsoid\Core\ContentMetadataCollector;
use Wikimedia\Parsoid\Core\LinkTarget;
use Wikimedia\Parsoid\Mocks\MockDataAccess;
use Wikimedia\Parsoid\Mocks\MockPageConfig;
use Wikimedia\Parsoid\Mocks\MockPageContent;
use Wikimedia\Parsoid\Mocks\MockSiteConfig;
use Wikimedia\Parsoid\Utils\Title;
use Wikimedia\Parsoid\Utils\Utils;


class SocketException extends \Exception {
    static function makeFromLastGlobal(string $strDoWhat): SocketException {
        $intErrno = \socket_last_error();
        $strMessage = \socket_strerror($intErrno);
        return new SocketException("failed to $strDoWhat: $strMessage", $intErrno);
    }

    static function makeFromLast(string $strDoWhat, Socket $objSocket): SocketException {
        $intErrno = \socket_last_error($objSocket);
        $strMessage = \socket_strerror($intErrno);
        return new SocketException("failed to $strDoWhat: $strMessage", $intErrno);
    }
}


class ShortReadException extends \Exception {
}
class WrongMagicException extends \Exception {
}


class ParseServerDataAccess extends MockDataAccess {
    private SiteConfig $siteConfig;
    public array $titleToTemplateData;
    private array $templateCache;

    public function __construct( SiteConfig $siteConfig, array $opts ) {
        $this->siteConfig = $siteConfig;
        $this->titleToTemplateData = [];
        $this->templateCache = [];
        parent::__construct($siteConfig, $opts);
    }

    /** @inheritDoc */
    public function parseWikitext(PageConfig $pageConfig, ContentMetadataCollector $metadata, string $wikitext): string {
        preg_match('#<([A-Za-z][^\t\n\v />\0]*)#', $wikitext, $match);
        $blnStrict = true;
        if (\in_array(\strtolower($match[1]), ['math', 'chem', 'timeline', 'syntaxhighlight', 'hiero', 'inputbox', 'score', 'graph', 'categorytree', 'maplink'], $blnStrict)) {
            return $wikitext;
        }

        return parent::parseWikitext($pageConfig, $metadata, $wikitext);
    }

    /**
     * @param string|LinkTarget $title
     * @return string
     */
    protected function wpsNormTitle( $title ): string {
        if ( is_int( $title ) ) {
            $title = "{$title}";
        }
        if ( !is_string( $title ) ) {
            $title = Title::newFromLinkTarget(
                $title, $this->siteConfig
            );
            return $title->getPrefixedDBKey();
        }
        return strtr( $title, ' ', '_' );
    }

    /** @inheritDoc */
    public function getPageInfo( $pageConfigOrTitle, array $titles ): array {
        $ret = [];
        foreach ( $titles as $title ) {
            // we copied this out only to change this line:
            $normTitle = $this->wpsNormTitle( $title );
            $pageData = self::$PAGE_DATA[$normTitle] ?? null;
            $ret[$title] = [
                'pageId' => $pageData['pageid'] ?? null,
                'revId' => $pageData['revid'] ?? null,
                'missing' => $pageData === null,
                'known' => $pageData !== null || ( $pageData['known'] ?? false ),
                'redirect' => $pageData['redirect'] ?? false,
                'linkclasses' => $pageData['linkclasses'] ?? [],
            ];
        }

        return $ret;
    }

    /** @inheritDoc */
    public function fetchTemplateSource(PageConfig $pageConfig, LinkTarget $title): ?PageContent {
        $normTitle = $this->wpsNormTitle( $title );
        if (\array_key_exists($normTitle, $this->templateCache)) {
            return $this->templateCache[$normTitle];
        }

        if (\array_key_exists($normTitle, $this->titleToTemplateData)) {
            $content = [
                "main" => $this->titleToTemplateData[$normTitle],
            ];
            $ret = new MockPageContent($content);
            $this->templateCache[$normTitle] = $ret;
            return $ret;
        } else {
            $content = [
                "main" => "",
            ];
            $ret = new MockPageContent($content);
            $this->templateCache[$normTitle] = $ret;
            return null;
        }
    }

    public function wpsSetTemplate($title, $content) {
        $normTitle = $this->wpsNormTitle($title);
        $this->titleToTemplateData[$normTitle] = $content;
    }
}

class ParseServerSiteConfig extends MockSiteConfig {
    protected $namespaceMap = [
        'media' => -2, 'medien' => -2,
        'special' => -1, 'spezial' => -1,
        '' => 0,
        'talk' => 1, 'diskussion' => 1,
        'user' => 2, 'benutzer' => 2,
        'user_talk' => 3, 'benutzer_diskussion' => 3,
        // Last one will be used by namespaceName
        'project' => 4, 'wp' => 4, 'wikipedia' => 4,
        'project_talk' => 5, 'wt' => 5, 'wikipedia_talk' => 5, 'wikipedia_diskussion' => 5,
        'file' => 6, 'datei' => 6,
        'file_talk' => 7, 'datei_diskussion' => 7,
        'category' => 14, 'kategorie' => 14,
        'category_talk' => 15, 'kategorie_diskussion' => 15,
    ];

    /** @inheritDoc */
    public function specialPageLocalName( string $alias ): ?string {
        return $alias;
    }
}


function makeSiteConfig(): ParseServerSiteConfig {
    $arrSiteConfigOpts = [];
    return new ParseServerSiteConfig($arrSiteConfigOpts);
}


function makeDataAccess(ParseServerSiteConfig $objSiteConfig): ParseServerDataAccess {
    $arrDataAccessConfigOpts = [];
    return new ParseServerDataAccess($objSiteConfig, $arrDataAccessConfigOpts);
}


function makeParsoid(ParseServerSiteConfig $objSiteConfig, ParseServerDataAccess $objDataAccess): Parsoid {
    return new Parsoid($objSiteConfig, $objDataAccess);
}


function recvExactly(Socket $objSock, int $intLength): string {
    $strWholeBuf = '';
    $strPartBuf = '';

    while (\strlen($strWholeBuf) < $intLength) {
        $intBytesReceived = \socket_recv($objSock, $strPartBuf, $intLength - \strlen($strWholeBuf), 0);
        if ($intBytesReceived === false) {
            throw SocketException::makeFromLast("recv", $objSock);
        } else if ($intBytesReceived === 0) {
            throw new ShortReadException();
        }
        $strWholeBuf .= $strPartBuf;
    }

    return $strWholeBuf;
}


function sendExactly(Socket $objSock, string $binData) {
    while (\strlen($binData) > 0) {
        $intBytesSent = \socket_send($objSock, $binData, \strlen($binData), 0);
        if ($intBytesSent === false) {
            throw SocketException::makeFromLast("send", $objSock);
        }
        $binData = \substr($binData, $intBytesSent);
    }
}


function bytesToInt32(string $binData): int {
    $intData = 0;
    for ($i = 0; $i < \strlen($binData); $i++) {
        $intDataByte = \ord($binData[$i]);
        $intData *= 256;
        $intData += $intDataByte;
    }
    return $intData;
}


function int32ToBytes(int $intData): string {
    $binData = "";
    for ($i = 0; $i < 4; $i++) {
        $binData = \chr($intData & 0xFF) . $binData;
        $intData = $intData >> 8;
    }
    return $binData;
}


function handleClient(Socket $objConn, ParseServerSiteConfig $objSiteConfig, ParseServerDataAccess $objDataAccess, Parsoid $objParsoid): bool {
    // read magic
    $strExpectedMagic = "WiKiCrUnCh";
    $strTemplateMagic = "WiKiTeMpL8";
    $strEndMagic = "EnOuGhWiKi";

    \assert(\strlen($strExpectedMagic) == \strlen($strEndMagic));

    $strReadMagic = recvExactly($objConn, strlen($strExpectedMagic));
    if ($strReadMagic === $strEndMagic) {
        // we are done :-)
        return false;
    }

    while ($strReadMagic === $strTemplateMagic) {
        // read title length and value
        $binTemplateTitleLength = recvExactly($objConn, 4);
        $intTemplateTitleLength = bytesToInt32($binTemplateTitleLength);
        $strTemplateTitle = recvExactly($objConn, $intTemplateTitleLength);

        // read template wikitext length and value
        $binTemplateLength = recvExactly($objConn, 4);
        $intTemplateLength = bytesToInt32($binTemplateLength);
        $strTemplate = recvExactly($objConn, $intTemplateLength);

        echo "Template '$strTemplateTitle' with $intTemplateLength bytes of body\n";

        $objDataAccess->wpsSetTemplate($strTemplateTitle, $strTemplate);

        // read next magic
        $strReadMagic = recvExactly($objConn, strlen($strExpectedMagic));
    }

    if ($strReadMagic !== $strExpectedMagic) {
        // sorry, nope
        throw new WrongMagicException();
    }

    // read title length and value
    $binTitleLength = recvExactly($objConn, 4);
    $intTitleLength = bytesToInt32($binTitleLength);
    $strTitle = recvExactly($objConn, $intTitleLength);

    // read wikitext length and value
    $binLength = recvExactly($objConn, 4);
    $intLength = bytesToInt32($binLength);
    $strWikitext = recvExactly($objConn, $intLength);

    echo "Article '$strTitle' with $intLength bytes of body\n";

    $arrPageOpts = [
        'title' => $strTitle,
    ];
    $objPageContent = new MockPageContent(['main' => ['content' => $strWikitext]]);
    $objPageConfig = new MockPageConfig($objSiteConfig, $arrPageOpts, $objPageContent);
    $arrParsoidOpts = [
        'body_only' => false,
        'wrapSections' => true,
        'nativeTemplateExpansion' => true, // https://gerrit.wikimedia.org/r/c/mediawiki/services/parsoid/+/1134341
    ];

    $strHtml = '';
    $numStart = \hrtime(true);
    try {
        $strHtml = $objParsoid->wikitext2html($objPageConfig, $arrParsoidOpts);
    } catch (\DOMException $ex) {
        // e.g. an angle bracket within a syntax highlighting block
        $strHtml = '';
    }
    $numEnd = \hrtime(true);
    $numDeltaNanosec = $numEnd - $numStart;
    $numDeltaSec = $numDeltaNanosec / (1000.0 * 1000.0 * 1000.0);
    echo "conversion to HTML took $numDeltaSec seconds\n";

    // send back the length
    $binHtmlLen = int32ToBytes(\strlen($strHtml));
    sendExactly($objConn, $binHtmlLen);

    // send back the HTML
    sendExactly($objConn, $strHtml);

    return true;
}


function runService(string $strListenIP, int $intPort) {
    // open a socket
    $objSock = \socket_create(AF_INET, SOCK_STREAM, SOL_TCP);
    if ($objSock === false) {
        throw SocketException::makeFromLastGlobal("create socket");
    }

    // TIME_WAIT was a mistake
    if (defined("SO_REUSEPORT")) {
        // allow reusing port
        \socket_set_option($objSock, SOL_SOCKET, SO_REUSEPORT, 1);
    } else if (defined("SO_REUSEADDR")) {
        // allow reusing socket address (address + port)
        \socket_set_option($objSock, SOL_SOCKET, SO_REUSEADDR, 1);
    }

    // bind
    if (!\socket_bind($objSock, $strListenIP, $intPort)) {
        throw SocketException::makeFromLast("bind", $objSock);
    }

    // listen
    if (!\socket_listen($objSock, 8)) {
        throw SocketException::makeFromLast("listen", $objSock);
    }

    while (($objConn = \socket_accept($objSock)) !== false) {
        // make a parsoid
        $objSiteConfig = makeSiteConfig();
        $objDataAccess = makeDataAccess($objSiteConfig);
        $objParsoid = makeParsoid($objSiteConfig, $objDataAccess);

        try {
            // handle the same client until we're done
            for (;;) {
                $blnRes = handleClient($objConn, $objSiteConfig, $objDataAccess, $objParsoid);
                if (!$blnRes) {
                    break;
                }
            }
        } catch (SocketException $exc) {
            echo "Socket exception: $exc\n";
        } catch (ShortReadException $exc) {
            echo "Short I/O exception: $exc\n";
        } catch (WrongMagicException $exc) {
            echo "Wrong magic value: $exc\n";
        }

        \socket_close($objConn);
    }

    $exc = SocketException::makeFromLast("accept", $objSock);
    \socket_close($objSock);
    throw $exc;
}

$arrArgs = $_SERVER["argv"];
if (\count($arrArgs) < 2 || \count($arrArgs) > 3) {
    echo "Usage: php wikiparseserver.php PORT [LISTENIP]\n";
    exit;
}

$intPort = (int)$arrArgs[1];
$strListenIP = \count($arrArgs) > 2 ? $arrArgs[2] : "127.0.0.1";
runService($strListenIP, $intPort);
